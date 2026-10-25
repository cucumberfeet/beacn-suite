use iced::advanced::svg::Handle;
use std::collections::HashMap;
use std::sync::LazyLock;

// Don't use this on WASM
#[cfg(not(target_arch = "wasm32"))]
pub mod runtime;

pub mod app;
pub mod events;
pub mod pages;

pub mod utility;
pub mod widgets;

pub static SVG: LazyLock<HashMap<&'static str, Handle>> = LazyLock::new(|| {
    let mut map = HashMap::default();
    map.insert(
        "hourglass",
        Handle::from_memory(include_bytes!("../../resources/ui/icons/hourglass.svg")),
    );
    map.insert(
        "mic",
        Handle::from_memory(include_bytes!("../../resources/ui/icons/microphone.svg")),
    );
    map.insert(
        "headphones",
        Handle::from_memory(include_bytes!("../../resources/ui/icons/headphones.svg")),
    );
    map.insert(
        "bulb",
        Handle::from_memory(include_bytes!("../../resources/ui/icons/lightbulb.svg")),
    );
    map.insert(
        "gear",
        Handle::from_memory(include_bytes!("../../resources/ui/icons/gear.svg")),
    );
    map.insert(
        "left_right",
        Handle::from_memory(include_bytes!("../../resources/ui/icons/left-right.svg")),
    );
    map.insert(
        "error",
        Handle::from_memory(include_bytes!("../../resources/ui/icons/error.svg")),
    );
    map.insert(
        "info",
        Handle::from_memory(include_bytes!("../../resources/ui/icons/info.svg")),
    );

    // Play / Pause / Record
    map.insert(
        "play",
        Handle::from_memory(include_bytes!("../../resources/ui/icons/play.svg")),
    );
    map.insert(
        "stop",
        Handle::from_memory(include_bytes!("../../resources/ui/icons/stop.svg")),
    );
    map.insert(
        "record",
        Handle::from_memory(include_bytes!("../../resources/ui/icons/record.svg")),
    );

    // EQ Modes
    map.insert(
        "eq_bell",
        Handle::from_memory(include_bytes!("../../resources/ui/eq/bell.svg")),
    );
    map.insert(
        "eq_high_pass",
        Handle::from_memory(include_bytes!("../../resources/ui/eq/high_pass.svg")),
    );
    map.insert(
        "eq_high_shelf",
        Handle::from_memory(include_bytes!("../../resources/ui/eq/high_shelf.svg")),
    );
    map.insert(
        "eq_low_pass",
        Handle::from_memory(include_bytes!("../../resources/ui/eq/low_pass.svg")),
    );
    map.insert(
        "eq_low_shelf",
        Handle::from_memory(include_bytes!("../../resources/ui/eq/low_shelf.svg")),
    );
    map.insert(
        "eq_notch",
        Handle::from_memory(include_bytes!("../../resources/ui/eq/notch.svg")),
    );

    // Utility Images
    map.insert(
        "link",
        Handle::from_memory(include_bytes!("../../resources/ui/icons/linked.svg")),
    );
    map.insert(
        "unlink",
        Handle::from_memory(include_bytes!("../../resources/ui/icons/unlinked.svg")),
    );

    // Pipeweaver Logo
    map.insert(
        "pipeweaver",
        Handle::from_memory(include_bytes!("../../resources/ui/pipeweaver.svg")),
    );

    // Mixer page icons (Font Awesome Free, see resources/ui/icons/README.md)
    map.insert(
        "mixer-angle-down",
        Handle::from_memory(include_bytes!(
            "../../resources/ui/icons/mixer/angle-down.svg"
        )),
    );
    map.insert(
        "mixer-angle-up",
        Handle::from_memory(include_bytes!(
            "../../resources/ui/icons/mixer/angle-up.svg"
        )),
    );
    map.insert(
        "mixer-check",
        Handle::from_memory(include_bytes!("../../resources/ui/icons/mixer/check.svg")),
    );
    map.insert(
        "mixer-ellipsis-vertical",
        Handle::from_memory(include_bytes!(
            "../../resources/ui/icons/mixer/ellipsis-vertical.svg"
        )),
    );
    map.insert(
        "mixer-eye",
        Handle::from_memory(include_bytes!("../../resources/ui/icons/mixer/eye.svg")),
    );
    map.insert(
        "mixer-eye-slash",
        Handle::from_memory(include_bytes!(
            "../../resources/ui/icons/mixer/eye-slash.svg"
        )),
    );
    map.insert(
        "mixer-grip-vertical",
        Handle::from_memory(include_bytes!(
            "../../resources/ui/icons/mixer/grip-vertical.svg"
        )),
    );
    map.insert(
        "mixer-link",
        Handle::from_memory(include_bytes!("../../resources/ui/icons/mixer/link.svg")),
    );
    map.insert(
        "mixer-link-slash",
        Handle::from_memory(include_bytes!(
            "../../resources/ui/icons/mixer/link-slash.svg"
        )),
    );
    map.insert(
        "mixer-palette",
        Handle::from_memory(include_bytes!("../../resources/ui/icons/mixer/palette.svg")),
    );
    map.insert(
        "mixer-pen",
        Handle::from_memory(include_bytes!("../../resources/ui/icons/mixer/pen.svg")),
    );
    map.insert(
        "mixer-plug",
        Handle::from_memory(include_bytes!("../../resources/ui/icons/mixer/plug.svg")),
    );
    map.insert(
        "mixer-plus",
        Handle::from_memory(include_bytes!("../../resources/ui/icons/mixer/plus.svg")),
    );
    map.insert(
        "mixer-rotate",
        Handle::from_memory(include_bytes!("../../resources/ui/icons/mixer/rotate.svg")),
    );
    map.insert(
        "mixer-sliders",
        Handle::from_memory(include_bytes!("../../resources/ui/icons/mixer/sliders.svg")),
    );
    map.insert(
        "mixer-thumbtack",
        Handle::from_memory(include_bytes!(
            "../../resources/ui/icons/mixer/thumbtack.svg"
        )),
    );
    map.insert(
        "mixer-trash",
        Handle::from_memory(include_bytes!("../../resources/ui/icons/mixer/trash.svg")),
    );
    map.insert(
        "mixer-volume-high",
        Handle::from_memory(include_bytes!(
            "../../resources/ui/icons/mixer/volume-high.svg"
        )),
    );
    map.insert(
        "mixer-volume-xmark",
        Handle::from_memory(include_bytes!(
            "../../resources/ui/icons/mixer/volume-xmark.svg"
        )),
    );
    map.insert(
        "mixer-xmark",
        Handle::from_memory(include_bytes!("../../resources/ui/icons/mixer/xmark.svg")),
    );

    map
});
