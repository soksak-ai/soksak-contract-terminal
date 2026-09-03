// A mode set before the stored output begins is in no byte the store holds, so a replay alone
// rebuilds a screen whose modes are the defaults. The mirror tracks mode state apart from the byte
// window — the corpus grades it on `private modes beyond the ring window` — and this is how that
// state leaves the mirror so the owner can record it.
//
// A serializer that omitted a mode a program negotiated is the failure this closes, and it is
// invisible until a program misbehaves against it. So the set is enumerated and compared, never
// assumed complete.
use soksak_contract_terminal::{ModeReport, Modes};

#[test]
fn a_report_holds_every_mode_the_canonical_form_holds() {
    let modes = Modes {
        bracketed_paste: true,
        app_cursor: true,
        app_keypad: false,
        mouse_click: true,
        mouse_drag: false,
        mouse_motion: true,
        sgr_mouse: true,
        utf8_mouse: false,
        focus_in_out: true,
        alternate_scroll: false,
        show_cursor: false,
        line_wrap: true,
        insert: true,
    };

    let report = ModeReport::of(modes, true);
    assert_eq!(report.modes, modes, "the report holds different modes than it was given");
    assert!(report.alt, "the report lost which screen the modes belong to");
}

// The report round-trips through the wire it travels on. A mode that survived the mirror and not the
// wire is one a restore silently drops.
#[test]
fn every_mode_survives_the_wire() {
    for index in 0..13 {
        let mut flags = [false; 13];
        flags[index] = true;
        let modes = Modes::from_flags(flags);
        let report = ModeReport::of(modes, index % 2 == 0);

        let encoded = report.encode();
        let back = ModeReport::decode(&encoded).expect("the report did not decode");

        assert_eq!(back, report, "mode {index} did not survive the wire");
    }
}

// A report a build did not write is refused rather than read as defaults. Reading it as defaults
// would restore a screen whose modes are wrong and state nothing about it.
#[test]
fn a_report_this_build_did_not_write_is_refused() {
    assert!(ModeReport::decode(b"").is_none(), "an empty report decoded");
    assert!(ModeReport::decode(b"v0 1 0").is_none(), "a report from another version decoded");
    assert!(ModeReport::decode(b"v1 nonsense").is_none(), "a malformed report decoded");
}

// The alternate screen has its own mode slots, so which screen a report is for is part of it. A
// report that lost that restores one screen's modes onto the other.
#[test]
fn the_screen_the_modes_belong_to_is_part_of_the_report() {
    let modes = Modes::from_flags([true; 13]);
    let primary = ModeReport::of(modes, false);
    let alternate = ModeReport::of(modes, true);

    assert_ne!(primary.encode(), alternate.encode(), "the two screens encode the same");
}
