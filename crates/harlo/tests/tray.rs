//! Harlo's tray icon as a desktop sees it: on a private session bus with a
//! stand-in tray host, never the desktop's own.

// Rust guideline compliant 2026-10-09

use std::collections::HashMap;
use std::io::{BufRead as _, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use harlo::tray::{Choice, Entry, Summary, Tray};
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{OwnedValue, Value};

/// How long the tray may take to register and update.
const WAIT: Duration = Duration::from_secs(10);

/// The tray host's side of the protocol, enough for an item to register.
struct Watcher {
    items: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

#[expect(
    clippy::unused_self,
    clippy::needless_pass_by_value,
    reason = "D-Bus methods take the shape zbus's interface macro requires"
)]
#[zbus::interface(name = "org.kde.StatusNotifierWatcher")]
impl Watcher {
    fn register_status_notifier_item(
        &self,
        service: &str,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) {
        // An item gives its well-known name, or its path on its own name.
        let name = if service.starts_with('/') {
            header.sender().map(ToString::to_string).unwrap_or_default()
        } else {
            service.to_owned()
        };
        if let Ok(mut items) = self.items.lock() {
            items.push(name);
        }
    }

    #[zbus(property)]
    fn is_status_notifier_host_registered(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn protocol_version(&self) -> i32 {
        0
    }

    #[zbus(property)]
    fn registered_status_notifier_items(&self) -> Vec<String> {
        self.items.lock().map(|i| i.clone()).unwrap_or_default()
    }
}

/// A private session bus, stopped when dropped.
struct Bus {
    daemon: Child,
    address: String,
}

impl Bus {
    fn start() -> Self {
        let mut daemon = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .spawn()
            .expect("dbus-daemon");
        let mut address = String::new();
        BufReader::new(daemon.stdout.take().expect("its output"))
            .read_line(&mut address)
            .expect("its address");
        Self {
            daemon,
            address: address.trim().to_owned(),
        }
    }
}

impl Drop for Bus {
    fn drop(&mut self) {
        // It may have ended already.
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
    }
}

/// A proxy that reads properties afresh each time, as the item announces
/// changes with its own signals rather than `PropertiesChanged`.
fn proxy<'a>(
    host: &Connection,
    destination: &'a str,
    path: &'a str,
    interface: &'a str,
) -> zbus::Result<Proxy<'a>> {
    zbus::blocking::proxy::Builder::new(host)
        .destination(destination)?
        .path(path)?
        .interface(interface)?
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
}

/// Waits for `check` to give something, or panics after [`WAIT`].
fn until<T>(what: &str, check: impl Fn() -> Option<T>) -> T {
    let deadline = Instant::now() + WAIT;
    loop {
        if let Some(found) = check() {
            return found;
        }
        assert!(Instant::now() < deadline, "{what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Icons as a tray item gives them: width, height and ARGB pixels.
type Pixmaps = Vec<(i32, i32, Vec<u8>)>;

/// A dbusmenu entry: its ID, properties and children.
type Layout = (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>);

/// The labels of a dbusmenu layout's children, with their IDs and whether
/// they can be chosen.
fn labels(layout: &Layout) -> Vec<(i32, String, bool)> {
    layout
        .2
        .iter()
        .filter_map(|child| {
            let (id, props, _): Layout = child.try_clone().ok()?.try_into().ok()?;
            let label = props
                .get("label")
                .and_then(|v| String::try_from(v.try_clone().ok()?).ok())
                .unwrap_or_default();
            let enabled = props
                .get("enabled")
                .and_then(|v| bool::try_from(v).ok())
                .unwrap_or(true);
            Some((id, label, enabled))
        })
        .collect()
}

#[test]
fn the_tray_shows_who_needs_you_and_its_menu_acts() {
    let bus = Bus::start();
    // SAFETY: this test is the only one in its binary, and nothing else in
    // it reads the environment while it's set.
    unsafe { std::env::set_var("DBUS_SESSION_BUS_ADDRESS", &bus.address) };

    let items = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let host = Connection::session().expect("the private bus");
    host.object_server()
        .at(
            "/StatusNotifierWatcher",
            Watcher {
                items: std::sync::Arc::clone(&items),
            },
        )
        .expect("the watcher");
    host.request_name("org.kde.StatusNotifierWatcher")
        .expect("the watcher's name");

    let (chose, chosen) = mpsc::channel();
    let mut tray = Tray::start(move |choice| {
        let _ = chose.send(choice);
    })
    .expect("a tray");
    let service = until("the tray registers", || items.lock().ok()?.first().cloned());
    let item = proxy(
        &host,
        service.as_str(),
        "/StatusNotifierItem",
        "org.kde.StatusNotifierItem",
    )
    .expect("the item");
    let status = || item.get_property::<String>("Status").ok();
    assert_eq!(status().as_deref(), Some("Active"));
    let icons: Pixmaps = item.get_property("IconPixmap").expect("icons");
    assert!(
        icons.iter().any(|(w, h, data)| *w == 32
            && *h == 32
            && data.len() == 32 * 32 * 4
            && data.chunks(4).any(|px| px[0] > 0)),
        "the logo, drawn"
    );

    // A session that needs the user makes it ask for attention.
    tray.show(Summary {
        waiting: vec![Entry {
            id: "ses_1".to_owned(),
            title: "fix_the build".to_owned(),
        }],
        working: vec![Entry {
            id: "ses_2".to_owned(),
            title: "Refactor".to_owned(),
        }],
    });
    until("it asks for attention", || {
        (status()? == "NeedsAttention").then_some(())
    });
    let tip: (String, Pixmaps, String, String) = item.get_property("ToolTip").expect("a tooltip");
    assert_eq!(tip.3, "1 session needs you · 1 working");

    let menu_path: zbus::zvariant::OwnedObjectPath = item.get_property("Menu").expect("a menu");
    let menu = proxy(
        &host,
        service.as_str(),
        menu_path.as_str(),
        "com.canonical.dbusmenu",
    )
    .expect("the menu");
    let (_revision, layout): (u32, Layout) = menu
        .call("GetLayout", &(0i32, -1i32, Vec::<&str>::new()))
        .expect("its layout");
    let entries = labels(&layout);
    let named: Vec<&str> = entries.iter().map(|(_, l, _)| l.as_str()).collect();
    assert_eq!(
        named,
        [
            "Needs you",
            "fix__the build",
            "",
            "Working",
            "Refactor",
            "",
            "Show Harlo",
            "Quit Harlo"
        ],
        "headings, sessions (underscores kept), then the actions"
    );
    assert!(!entries[0].2, "a heading isn't a choice");

    // Clicking entries reports them.
    let click = |label: &str| {
        let id = entries
            .iter()
            .find(|(_, l, _)| l == label)
            .map(|(id, _, _)| *id)
            .expect("the entry");
        let _: () = menu
            .call("Event", &(id, "clicked", Value::from(0i32), 0u32))
            .expect("clicked");
    };
    click("fix__the build");
    assert_eq!(
        chosen.recv_timeout(WAIT),
        Ok(Choice::Open("ses_1".to_owned()))
    );
    click("Quit Harlo");
    assert_eq!(chosen.recv_timeout(WAIT), Ok(Choice::Quit));
    // A click on the icon itself shows the window.
    let _: () = item.call("Activate", &(0i32, 0i32)).expect("activated");
    assert_eq!(chosen.recv_timeout(WAIT), Ok(Choice::Show));
}
