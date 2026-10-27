#[cfg(target_arch = "wasm32")]
use tokio_with_wasm as tokio;

/*
  This file primarily manages hot-plugging opening devices, and device messaging.

  When a device appears, we open it, create a message handler and throw it upstream. We then
  listen on all the message handlers, and when one pops up, handle the message.

  If a device disappears, we simply drop its channel, upstream should pick up on that and
  handle it appropriately.

  For the moment, for the Beacn Mic + Beacn Studio we're going to have a single message type,
  same applies for the Mix and Mix Create. The devices are too similar to have to worry about
  differences.
*/
use crate::ManagerMessages;
use crate::devices::states::audio::AudioState;
use crate::devices::states::control::ControlState;
use crate::integrations::pipeweaver::spawn_pipeweaver_handler;
use crate::managers::LoginEventTriggers;
use anyhow::anyhow;
use beacn_lib::audio::data::BulkMessage as BMessage;
use beacn_lib::audio::messages::Message as AMessage;
use beacn_lib::audio::{BeacnAudioDevice, LinkedApp, open_audio_device};
use beacn_lib::controller::messages::Message as CMessage;
use beacn_lib::controller::{BeacnControlDevice, open_control_device_with_timer};
use beacn_lib::flume::{Receiver, Sender, bounded, unbounded};
use beacn_lib::manager::{
    DeviceLocation, DeviceType, HotPlugMessage, HotPlugThreadManagement, get_beacn_mic_devices,
    get_beacn_mix_create_device, get_beacn_mix_device, get_beacn_studio_devices,
    watch_hotplug_devices,
};
use beacn_lib::version::VersionNumber;
use beacn_lib::{BeacnError, UsbError};
use iced::futures::FutureExt;
use log::{debug, error, warn};
use std::collections::{HashMap, HashSet};
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use strum_macros::Display;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use web_time::Duration;

/// Starts beacn-lib's hotplug watcher, which first reports every device already present and
/// then follows hotplug events. Each watcher gets its own channels, so a replaced one can't
/// stop the manager when it winds down.
fn start_hotplug_watch() -> (Receiver<HotPlugMessage>, Sender<HotPlugThreadManagement>) {
    let (plug_tx, plug_rx) = unbounded();
    let (manage_tx, manage_rx) = unbounded();
    tokio::spawn(watch_hotplug_devices(plug_tx, manage_rx));
    (plug_rx, manage_tx)
}

/// Every Beacn device the kernel has right now, whatever the hotplug watcher has reported
async fn present_beacn_devices() -> HashSet<DeviceLocation> {
    let mut present = HashSet::new();
    present.extend(get_beacn_mic_devices().await);
    present.extend(get_beacn_studio_devices().await);
    present.extend(get_beacn_mix_device().await);
    present.extend(get_beacn_mix_create_device().await);
    present
}

/// Settings we replay to a control device after a wake, keyed so newer values replace older
fn setting_key(message: &CMessage) -> Option<String> {
    match message {
        CMessage::DisplayBrightness(_) => Some("display_brightness".into()),
        CMessage::ButtonBrightness(_) => Some("button_brightness".into()),
        CMessage::DisplayDimTime(_) => Some("display_dim".into()),
        CMessage::ButtonColour(button, _) => Some(format!("colour_{button:?}")),
        _ => None,
    }
}

/// How often we compare the devices we know with the devices that are really there
const RECONCILE_PERIOD: Duration = Duration::from_secs(2);

/// Never re-scan more often than this, whatever the checks say
const MIN_RESCAN_GAP: Duration = Duration::from_secs(15);

/// After a wake or unlock, the screens get switched on, their settings re-sent and a full frame
/// pushed again at these delays, in case the device was still resetting the first time
const WAKE_REPUSH: [Duration; 2] = [Duration::from_secs(3), Duration::from_secs(10)];

pub(crate) async fn spawn_device_manager(
    self_rx: Receiver<ManagerMessages>,
    event_tx: Sender<DeviceMessage>,
) {
    #[cfg_attr(not(target_os = "linux"), allow(unused))]
    let (login_tx, login_rx) = bounded(5);

    #[cfg_attr(not(target_os = "linux"), allow(unused))]
    let (login_stop_tx, login_stop_rx) = tokio::sync::mpsc::channel(1);

    // Device state, keyed by location.
    let mut devices: HashMap<DeviceLocation, DeviceEntry> = HashMap::new();

    // Small List of forwarding tasks
    let (device_event_tx, device_event_rx) = unbounded();
    let mut forwarders: HashMap<DeviceLocation, JoinHandle<()>> = HashMap::new();

    // watch_hotplug_devices is beacn-lib's async-native hotplug watcher -- spawn it as a
    // task instead of spawn_hotplug_handler, which would give us a dedicated OS thread we
    // don't need now that we're on a runtime.
    let (mut plug_rx, mut manage_tx) = start_hotplug_watch();

    // Every location the watcher has reported (opened or not) and not yet removed
    let mut attached: HashSet<DeviceLocation> = HashSet::new();

    // The hotplug watcher can miss events: when Balthasar wakes, its USB controllers are
    // re-bound and dozens of devices re-enumerate at once, and the add events for the Beacn
    // devices got lost (the Mix Create's screen then stays black until it's replugged). So we
    // also check what's really there, and start a fresh watcher (a full re-enumeration) when
    // the two disagree.
    let mut reconcile = tokio::time::interval(RECONCILE_PERIOD);
    reconcile.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut mismatches = 0u8;
    let mut last_rescan: Option<tokio::time::Instant> = None;

    // Last settings sent to each control device, and when to push them (and a frame) again
    let mut settings: HashMap<DeviceLocation, HashMap<String, CMessage>> = HashMap::new();
    let mut repush_at: Vec<tokio::time::Instant> = vec![];
    let mut locked = false;

    #[cfg(target_os = "linux")]
    {
        use crate::managers::login::spawn_login_handler;
        tokio::spawn(spawn_login_handler(login_tx, login_stop_rx));
    }

    let mut suspended = false;
    let mut pending_attachments: Vec<(DeviceLocation, DeviceType, Sender<()>)> = vec![];

    loop {
        tokio::select! {
            msg = self_rx.recv_async() => {
                match msg {
                    Ok(ManagerMessages::Quit) | Err(_) => break,
                }
            }

            msg = login_rx.recv_async() => {
                let Ok(msg) = msg else { break };
                debug!("Received Login State Message: {msg:?}");

                match msg {
                    LoginEventTriggers::Sleep(tx) => {
                        suspended = true;
                        set_pipeweaver_draw_suspended(&devices, true);
                        enable_devices(&devices, false).await;
                        let _ = tx.send(());
                    }

                    LoginEventTriggers::Wake(tx) => {
                        suspended = false;
                        mismatches = 0;
                        reconcile.reset();
                        let now = tokio::time::Instant::now();
                        repush_at = WAKE_REPUSH.iter().map(|d| now + *d).collect();

                        for (location, device_type, health_tx) in pending_attachments.drain(..) {
                            handle_device_attached(
                                location,
                                device_type,
                                health_tx,
                                &mut devices,
                                &mut forwarders,
                                &device_event_tx,
                                &event_tx
                            )
                            .await;
                        }

                        set_pipeweaver_draw_suspended(&devices, false);
                        enable_devices(&devices, true).await;
                        let _ = tx.send(());
                    }

                    LoginEventTriggers::Lock => {
                        locked = true;
                        set_pipeweaver_draw_suspended(&devices, true);
                        enable_devices(&devices, false).await;
                    }

                    LoginEventTriggers::Unlock => {
                        locked = false;
                        set_pipeweaver_draw_suspended(&devices, false);
                        enable_devices(&devices, true).await;
                        let now = tokio::time::Instant::now();
                        repush_at = WAKE_REPUSH.iter().map(|d| now + *d).collect();
                    }
                }
            }

            msg = plug_rx.recv_async() => {
                let Ok(msg) = msg else { break };

                match msg {
                    HotPlugMessage::DeviceAttached(location, device_type, health_tx) => {
                        attached.insert(location.clone());
                        if suspended {
                            pending_attachments.push((location, device_type, health_tx));
                        } else {
                            handle_device_attached(
                                location,
                                device_type,
                                health_tx,
                                &mut devices,
                                &mut forwarders,
                                &device_event_tx,
                                &event_tx,
                            )
                            .await;
                        }
                    }

                    HotPlugMessage::DeviceRemoved(location) => {
                        attached.remove(&location);
                        pending_attachments.retain(|(loc, _, _)| *loc != location);
                        remove_device(&location, &mut devices, &mut forwarders, &mut settings, &event_tx);
                    }

                    HotPlugMessage::ThreadStopped => break,
                }
            }

            _ = reconcile.tick() => {
                if suspended {
                    continue;
                }

                let now = tokio::time::Instant::now();
                if repush_at.first().is_some_and(|at| *at <= now) {
                    repush_at.remove(0);
                    if !locked {
                        repush_displays(&devices, &settings).await;
                    }
                }

                let present = present_beacn_devices().await;
                if present == attached {
                    mismatches = 0;
                    continue;
                }

                // A normal plug or unplug shows up here for a moment before its event
                // arrives, so only act when it's still different on the next check
                mismatches += 1;
                if mismatches < 2 || last_rescan.is_some_and(|at| at.elapsed() < MIN_RESCAN_GAP) {
                    continue;
                }
                mismatches = 0;
                last_rescan = Some(now);

                let missing: Vec<String> = present.difference(&attached).map(|l| l.to_string()).collect();
                let gone: Vec<String> = attached.difference(&present).map(|l| l.to_string()).collect();
                warn!(
                    "USB devices changed without hotplug events (new: {missing:?}, gone: {gone:?}), re-scanning"
                );

                // Replace the watcher and reopen everything from scratch
                let _ = manage_tx.send(HotPlugThreadManagement::Quit);
                for location in attached.drain() {
                    remove_device(&location, &mut devices, &mut forwarders, &mut settings, &event_tx);
                }
                pending_attachments.clear();
                (plug_rx, manage_tx) = start_hotplug_watch();
            }

            Ok((location, req)) = device_event_rx.recv_async() => {
                match req {
                    DeviceRequest::Audio(msg) => {
                        if let Some(DeviceEntry::Audio(dev)) = devices.get(&location) {
                            match msg {
                                AudioMessage::Sync => {
                                    event_tx.send_async(DeviceMessage::AudioSyncHandled).await.unwrap();
                                }

                                AudioMessage::Send(msg) => {
                                    let response = AssertUnwindSafe(dev.handle_message(msg)).catch_unwind().await;
                                    let result = match response {
                                        Ok(result) => {
                                            match &result {
                                                Ok(msg) => Ok(*msg),
                                                Err(e) => Err(e.to_string()),
                                            }
                                        }

                                        Err(panic) => {
                                            let err = panic.downcast_ref::<String>().cloned().unwrap_or_else(|| "Unknown Error".to_string());
                                            Err(err.clone())
                                        }
                                    };

                                    // Send the response, UI will handle this..
                                    let message = DeviceMessage::AudioMessageHandled(location.clone(), msg, result);
                                    event_tx.send_async(message).await.unwrap();
                                }

                                AudioMessage::SendBulk(msg) => {
                                    let response = AssertUnwindSafe(dev.handle_bulk_message(msg)).catch_unwind().await;
                                    let result = match response {
                                        Ok(result) => {
                                            match &result {
                                                Ok(msg) => Ok(*msg),
                                                Err(e) => Err(e.to_string()),
                                            }
                                        }

                                        Err(panic) => {
                                            let err = panic.downcast_ref::<String>().cloned().unwrap_or_else(|| "Unknown Error".to_string());
                                            Err(err.clone())
                                        }
                                    };

                                    // Send the response, UI will handle this..
                                    let message = DeviceMessage::BulkMessageHandled(location.clone(), result);
                                    event_tx.send_async(message).await.unwrap();
                                }

                                AudioMessage::Handle(msg, resp) => {
                                    let response = AssertUnwindSafe(dev.handle_message(msg)).catch_unwind().await;

                                    match response {
                                        Ok(result) => {
                                            let _ = resp.send(result);
                                        }

                                        Err(panic) => {
                                            let error = panic.downcast_ref::<String>().cloned().unwrap_or_else(|| "Unknown Error".to_string());
                                            let _ = resp.send(Err(anyhow!(error).into()));
                                        }
                                    }
                                }

                                AudioMessage::Linked(command) => match command {
                                    LinkedCommands::GetLinked(tx) => {
                                        let _ = tx.send(dev.get_linked_apps().await);
                                    }

                                    LinkedCommands::SetLinked(app, tx) => {
                                        let _ = tx.send(dev.set_linked_app(app).await);
                                    }
                                },
                            }
                        }
                    }

                    DeviceRequest::Control(msg) => {
                        if let Some(DeviceEntry::Control(dev, ..)) = devices.get(&location) {
                            match msg {
                                ControlMessage::Handle(msg, resp) => {
                                    if let Some(key) = setting_key(&msg) {
                                        settings.entry(location.clone()).or_default().insert(key, msg.clone());
                                    }
                                    let response = AssertUnwindSafe(dev.handle_message(msg)).catch_unwind().await;

                                    match response {
                                        Ok(result) => {
                                            let _ = resp.send(result);
                                        }

                                        Err(panic) => {
                                            let error = panic.downcast_ref::<String>().cloned().unwrap_or_else(|| "Unknown Error".to_string());
                                            let _ = resp.send(Err(anyhow!(error).into()));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Stop the dbus login handler
    let _ = login_stop_tx.send(()).await;

    // Stop any control devices which may be active
    for device in devices.values() {
        if let DeviceEntry::Control(_, stop, _, _) = device {
            let _ = stop.send(true);
        }
    }

    // Drain the devices until they're finished. No more `runtime().block_on(...)` needed
    // here -- we're already running on the runtime, this is just the tail of the same
    // async fn.
    loop {
        let all_done = devices.values().all(|d| match d {
            DeviceEntry::Control(_, _, _, task) => task.is_finished(),
            _ => true,
        });
        if all_done {
            break;
        }

        tokio::select! {
            Ok((location, req)) = device_event_rx.recv_async() => {
                if let DeviceRequest::Control(msg) = req
                    && let Some(DeviceEntry::Control(dev, ..)) = devices.get(&location) {
                        match msg {
                            ControlMessage::Handle(msg, resp) => {
                                let _ = resp.send(dev.handle_message(msg).await);
                            }
                        }
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(10)) => {}
        }
    }

    // For some reason, we're stopping. If the manager channel is still open, tell it to stop.
    let _ = manage_tx.send(HotPlugThreadManagement::Quit);
    debug!("Device Manager Stopped");
}

#[allow(clippy::too_many_arguments)]
async fn handle_device_attached(
    location: DeviceLocation,
    device_type: DeviceType,
    health_tx: Sender<()>,
    devices: &mut HashMap<DeviceLocation, DeviceEntry>,
    forwarders: &mut HashMap<DeviceLocation, JoinHandle<()>>,
    device_event_tx: &Sender<(DeviceLocation, DeviceRequest)>,
    event_tx: &Sender<DeviceMessage>,
) {
    match device_type {
        DeviceType::BeacnMic | DeviceType::BeacnStudio => {
            let (device, state) = match open_audio_device(location.clone()).await {
                Ok(d) => (Some(d), DefinitionState::Running),
                Err(e) => {
                    error!("Failed to open audio device: {e}");
                    (
                        None,
                        DefinitionState::Error(match e {
                            BeacnError::Usb(UsbError::PermissionDenied) => {
                                ErrorType::PermissionDenied
                            }
                            BeacnError::Usb(UsbError::Busy) => ErrorType::ResourceBusy,
                            BeacnError::Usb(e) => ErrorType::Other(format!("{:?}", e)),
                            BeacnError::Other(e) => ErrorType::Other(e.to_string()),
                        }),
                    )
                }
            };

            let (serial, version) = match &device {
                Some(d) => (d.get_serial(), d.get_version()),
                None => ("Unknown".to_string(), VersionNumber(0, 0, 0, 0)),
            };

            // Firstly, build the device definition
            let data = DeviceDefinition {
                state,
                location: location.clone(),
                device_type,
                device_info: DeviceInfo { serial, version },
            };

            // Create a Message Bus for it
            let (tx, rx) = unbounded();

            // Add this into our device map, and spawn a task to forward its requests
            if let Some(device) = device {
                devices.insert(location.clone(), DeviceEntry::Audio(device));
                forwarders.insert(
                    location.clone(),
                    spawn_forwarder(location, rx, device_event_tx.clone(), DeviceRequest::Audio),
                );
            }

            // Create an initial state for the device, and send it.
            let mut state = AudioState::new(data, tx);
            let arrived = DeviceArriveMessage::Audio(state.clone());
            let message = DeviceMessage::DeviceArrived(arrived);
            event_tx.send_async(message).await.unwrap();

            // Clone the Event TX so we can move it into the future...
            let event_tx = event_tx.clone();
            tokio::spawn(async move {
                // Load the settings, then re-send the arrival.
                state.load_settings().await;
                let arrived = DeviceArriveMessage::Audio(state);
                let message = DeviceMessage::DeviceArrived(arrived);
                event_tx.send_async(message).await.unwrap();
            });
        }
        DeviceType::BeacnMix | DeviceType::BeacnMixCreate => {
            // This is relatively similar, but the code paths are different. In
            // the future, we'd be setting up button handlers, a pipeweaver
            // connection and management.
            let (input_tx, input_rx) = unbounded();

            let loc = location.clone();
            let input_tx = Some(input_tx);
            let poll_timer = Duration::from_millis(10);
            let (device, state) =
                match open_control_device_with_timer(loc, input_tx, health_tx, poll_timer).await {
                    Ok(d) => (Some(d), DefinitionState::Running),
                    Err(e) => {
                        error!("Failed to open control device: {e}");
                        (
                            None,
                            DefinitionState::Error(match e {
                                BeacnError::Usb(UsbError::PermissionDenied) => {
                                    ErrorType::PermissionDenied
                                }
                                BeacnError::Usb(UsbError::Busy) => ErrorType::ResourceBusy,
                                BeacnError::Usb(e) => ErrorType::Other(format!("{:?}", e)),
                                BeacnError::Other(e) => ErrorType::Other(e.to_string()),
                            }),
                        )
                    }
                };

            let (serial, version) = match &device {
                Some(d) => (d.get_serial(), d.get_version()),
                None => ("Unknown".to_string(), VersionNumber(0, 0, 0, 0)),
            };

            let data = DeviceDefinition {
                state,
                location: location.clone(),
                device_type,
                device_info: DeviceInfo { serial, version },
            };

            let (tx, rx) = unbounded();
            let (stop_tx, stop_rx) = watch::channel(false);
            let (suspended_tx, suspended_rx) = watch::channel(false);
            let img_tx = tx.clone();
            let task =
                spawn_pipeweaver_handler(img_tx, device_type, input_rx, stop_rx, suspended_rx);

            if let Some(device) = device {
                devices.insert(
                    location.clone(),
                    DeviceEntry::Control(device, stop_tx, suspended_tx, task),
                );
                forwarders.insert(
                    location.clone(),
                    spawn_forwarder(
                        location,
                        rx,
                        device_event_tx.clone(),
                        DeviceRequest::Control,
                    ),
                );
            }

            // Reserve a Slot in the Queue
            let mut state = ControlState::new(data, tx);
            let arrived = DeviceArriveMessage::Control(state.clone());
            let message = DeviceMessage::DeviceArrived(arrived);
            let _ = event_tx.send_async(message).await;

            let event_tx = event_tx.clone();
            tokio::spawn(async move {
                state.load_settings_async().await;

                let arrived = DeviceArriveMessage::Control(state);
                let message = DeviceMessage::DeviceArrived(arrived);
                let _ = event_tx.send_async(message).await;
            });
        }
    }
}

/// Spawn a small task that just loops on `rx` and forwards everything it receives into
/// the shared `device_event_tx`, tagged with `location` and wrapped into the common
/// `DeviceRequest` enum via `wrap` (`DeviceRequest::Audio` / `DeviceRequest::Control`).
/// Exits on its own once `rx`'s channel closes; also explicitly `.abort()`ed on device
/// removal so it doesn't linger.
fn spawn_forwarder<M: Send + 'static>(
    location: DeviceLocation,
    rx: Receiver<M>,
    device_event_tx: Sender<(DeviceLocation, DeviceRequest)>,
    wrap: impl Fn(M) -> DeviceRequest + Send + 'static,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        while let Ok(msg) = rx.recv_async().await {
            if device_event_tx
                .send_async((location.clone(), wrap(msg)))
                .await
                .is_err()
            {
                break;
            }
        }
    })
}

enum DeviceRequest {
    Audio(AudioMessage),
    Control(ControlMessage),
}

#[allow(unused)]
async fn enable_devices(devices: &HashMap<DeviceLocation, DeviceEntry>, enabled: bool) {
    for device in devices.values() {
        if let DeviceEntry::Control(dev, ..) = device {
            let message = CMessage::Enabled(enabled);
            let _ = dev.handle_message(message).await;
        }
    }
}

fn remove_device(
    location: &DeviceLocation,
    devices: &mut HashMap<DeviceLocation, DeviceEntry>,
    forwarders: &mut HashMap<DeviceLocation, JoinHandle<()>>,
    settings: &mut HashMap<DeviceLocation, HashMap<String, CMessage>>,
    event_tx: &Sender<DeviceMessage>,
) {
    let _ = event_tx.send(DeviceMessage::DeviceRemoved(location.clone()));
    devices.remove(location);
    settings.remove(location);
    if let Some(forwarder) = forwarders.remove(location) {
        forwarder.abort();
    }
}

/// Switches every control device's screen on, re-sends its settings and has Pipeweaver's
/// screen handler push a full frame. Harmless when nothing was wrong.
async fn repush_displays(
    devices: &HashMap<DeviceLocation, DeviceEntry>,
    settings: &HashMap<DeviceLocation, HashMap<String, CMessage>>,
) {
    for (location, device) in devices {
        if let DeviceEntry::Control(dev, _, draw_suspend, _) = device {
            debug!("Re-pushing the screen of {location}");
            let _ = dev.handle_message(CMessage::Enabled(true)).await;
            for message in settings.get(location).into_iter().flat_map(|s| s.values()) {
                let _ = dev.handle_message(message.clone()).await;
            }
            // Sending even an unchanged value makes the handler invalidate and redraw
            let _ = draw_suspend.send(false);
        }
    }
}

fn set_pipeweaver_draw_suspended(devices: &HashMap<DeviceLocation, DeviceEntry>, suspended: bool) {
    for device in devices.values() {
        if let DeviceEntry::Control(_, _, draw_suspend, _) = device {
            let _ = draw_suspend.send(suspended);
        }
    }
}

enum DeviceEntry {
    Audio(Box<dyn BeacnAudioDevice>),
    Control(
        Arc<Box<dyn BeacnControlDevice>>,
        watch::Sender<bool>,
        watch::Sender<bool>,
        JoinHandle<()>,
    ),
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub(crate) enum DeviceMessage {
    DeviceArrived(DeviceArriveMessage),
    DeviceRemoved(DeviceLocation),

    // A sync message is just like it sounds, a message which is called from the frontend to the
    // manager, then back to the frontend. It's useful for when you're sending bulk async messages,
    // because it'll be sent once *EVERYTHING* ahead of it has been handled, so if you're dependent
    // on a value change something, this is how you know it's ready.
    AudioSyncHandled,
    AudioMessageHandled(DeviceLocation, AMessage, Result<AMessage, String>),
    BulkMessageHandled(DeviceLocation, Result<BMessage, String>),
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub(crate) enum DeviceArriveMessage {
    Audio(AudioState),
    Control(ControlState),
}

#[derive(Debug)]
pub enum AudioMessage {
    Sync,
    Send(AMessage),
    SendBulk(BMessage),

    Handle(AMessage, oneshot::Sender<Result<AMessage, BeacnError>>),
    Linked(LinkedCommands),
}

#[derive(Debug)]
pub enum LinkedCommands {
    GetLinked(oneshot::Sender<Result<Option<Vec<LinkedApp>>, BeacnError>>),
    SetLinked(LinkedApp, oneshot::Sender<Result<(), BeacnError>>),
}

#[allow(unused)]
pub enum ControlMessage {
    Handle(CMessage, oneshot::Sender<Result<CMessage, BeacnError>>),
}

#[derive(Debug, Default, Clone, Hash, PartialEq, Eq)]
pub struct DeviceDefinition {
    pub state: DefinitionState,
    pub location: DeviceLocation,
    pub device_type: DeviceType,
    pub device_info: DeviceInfo,
}

#[derive(Debug, Default, Clone, Hash, PartialEq, Eq)]
pub struct DeviceInfo {
    pub serial: String,
    pub version: VersionNumber,
}

#[derive(Debug, Default, Clone, Hash, PartialEq, Eq)]
pub enum DefinitionState {
    #[default]
    Running,
    Error(ErrorType),
}

#[derive(Display, Debug, Default, Clone, Hash, PartialEq, Eq)]
pub enum ErrorType {
    PermissionDenied,
    ResourceBusy,
    Other(String),
    #[default]
    Unknown,
}
