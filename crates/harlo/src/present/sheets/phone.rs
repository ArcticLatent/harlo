//! Settings → Phone: phone access, the relay, pairing a phone, and the
//! paired phones.

// Rust guideline compliant 2026-10-09

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use harlo_core::phone::RelayStatus;
use slint::{Image, Rgba8Pixel, SharedPixelBuffer};

use super::{code_pair, heading, int, note, pair, switch};
use crate::app::{App, Setting};
use crate::ui::SheetRow;
use crate::window;

/// Blank modules around the QR code, as its readers expect.
const QUIET_ZONE: usize = 4;

/// Pixels per module of the QR code.
const MODULE: usize = 6;

/// The section's rows.
pub(super) fn phone_rows(app: &App) -> Vec<SheetRow> {
    let state = &app.phone;
    let on = app.preferences.phone;
    let mut rows = vec![
        heading("Access"),
        note("Follow sessions and answer agents from your phone, encrypted end to end."),
        switch(
            Setting::Phone,
            "Phone access",
            "On this computer's networks.",
            on,
        ),
    ];
    if on {
        rows.push(match (&state.running, &state.error) {
            (Some(running), _) => {
                let connected = state.connected.len();
                let what = match connected {
                    0 => format!("Port {}", running.address().port()),
                    1 => format!("Port {} · 1 phone connected", running.address().port()),
                    n => format!("Port {} · {n} phones connected", running.address().port()),
                };
                code_pair("Listening", &what, 1)
            }
            (None, Some(why)) => pair("Not started", why, 2),
            (None, None) => pair("Status", "Starting…", 3),
        });
    }
    rows.push(switch(
        Setting::PhoneRelay,
        "Reach Harlo from anywhere",
        "Through Harlo's relay, which can't read the messages.",
        app.preferences.phone_relay,
    ));
    if on && app.preferences.phone_relay {
        rows.push(match &state.relay_status {
            Some(RelayStatus::Online(_)) => pair("Relay", "Connected", 1),
            Some(RelayStatus::Failed(why)) => pair("Relay", why, 2),
            Some(RelayStatus::Connecting) | None => pair("Relay", "Connecting…", 3),
        });
    }

    rows.push(heading("Pair a phone"));
    pairing_rows(app, &mut rows);

    rows.push(heading("Paired phones"));
    device_rows(app, &mut rows);
    rows
}

/// The pairing code, or the button that shows one.
fn pairing_rows(app: &App, rows: &mut Vec<SheetRow>) {
    let state = &app.phone;
    if state.running.is_none() {
        rows.push(note("Turn on phone access to pair a phone."));
        return;
    }
    let Some(shown) = &state.shown else {
        let mut row = pair("Pairing code", "Shown here for the Harlo app to scan", 3);
        row.action = "Show a code".into();
        row.setting = window::PAIR_PHONE;
        rows.push(row);
        return;
    };
    rows.push(SheetRow {
        kind: 10,
        name: "Pairing QR code".into(),
        picture: qr_image(&shown.text),
        ..SheetRow::default()
    });
    rows.push(note(
        "Scan it with the Harlo app. It works once, for ten minutes.",
    ));
    let mut copy = pair("Pairing link", "To paste in the app instead", 3);
    copy.action = "Copy".into();
    copy.copy = shown.text.as_str().into();
    // Copying is all it does.
    copy.setting = -1;
    rows.push(copy);
}

/// The paired phones, each with its Remove button, and the test notification.
fn device_rows(app: &App, rows: &mut Vec<SheetRow>) {
    let state = &app.phone;
    if state.devices.is_empty() {
        rows.push(note("No phones yet."));
        return;
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    for (index, device) in state.devices.iter().enumerate() {
        let mut row = if state.connected.contains(&device.id) {
            pair(&device.name, "Connected", 1)
        } else {
            let seen = u64::try_from(device.last_seen).unwrap_or(0);
            let age = Duration::from_secs(now.saturating_sub(seen));
            let when = harlo_core::sessions::compact_age(age);
            pair(&device.name, &format!("Last connected {when} ago"), 3)
        };
        if state.running.is_some() {
            row.action = "Remove".into();
            row.setting = window::REMOVE_PHONE;
            row.choice = int(index);
        }
        rows.push(row);
    }
    let pushes = state.devices.iter().filter(|d| d.push.is_some()).count();
    if state.relay.is_some() && pushes > 0 {
        let mut row = pair(
            "Notifications",
            &format!("{pushes} of them take notifications"),
            3,
        );
        row.action = "Send a test".into();
        row.setting = window::TEST_PHONE;
        rows.push(row);
        match &state.tested {
            Some(Ok(n)) => rows.push(pair("Test", &format!("Sent to {n}"), 1)),
            Some(Err(why)) => rows.push(pair("Test", why, 2)),
            None => {}
        }
    }
}

/// Draws `text` as a QR code, black on white, with its quiet zone.
fn qr_image(text: &str) -> Image {
    let Ok(code) = qrcode::QrCode::new(text.as_bytes()) else {
        return Image::default();
    };
    let width = code.width();
    let colors = code.to_colors();
    let side = (width + 2 * QUIET_ZONE) * MODULE;
    let Ok(pixels) = u32::try_from(side) else {
        return Image::default();
    };
    let mut buffer = SharedPixelBuffer::<Rgba8Pixel>::new(pixels, pixels);
    let white = Rgba8Pixel::new(255, 255, 255, 255);
    let black = Rgba8Pixel::new(0, 0, 0, 255);
    let data = buffer.make_mut_slice();
    data.fill(white);
    for (i, color) in colors.iter().enumerate() {
        if *color != qrcode::Color::Dark {
            continue;
        }
        let (column, line) = (i % width + QUIET_ZONE, i / width + QUIET_ZONE);
        for y in line * MODULE..(line + 1) * MODULE {
            let start = y * side + column * MODULE;
            data[start..start + MODULE].fill(black);
        }
    }
    Image::from_rgba8(buffer)
}
