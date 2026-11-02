//! Tray icon on the Linux desktop: a StatusNotifierItem (KDE Plasma, Xfce, Cinnamon, MATE,
//! Budgie; GNOME with the AppIndicator extension) with its menu (com.canonical.dbusmenu),
//! and desktop notifications — all through our own D-Bus client.

use super::dbus::{Bus, Msg, CALL, SIGNAL, V};
use std::sync::{Arc, Mutex};

const ITEM_PATH: &str = "/StatusNotifierItem";
const MENU_PATH: &str = "/MenuBar";
const ITEM_IFACE: &str = "org.kde.StatusNotifierItem";
const MENU_IFACE: &str = "com.canonical.dbusmenu";
const PROPS: &str = "org.freedesktop.DBus.Properties";
const WATCHER: &str = "org.kde.StatusNotifierWatcher";
const NOTIFICATIONS: &str = "org.freedesktop.Notifications";

/// Click on the icon itself (not on the menu).
pub const ACTIVATE: i32 = -1;
/// Click on our notification (or its "Settings" button).
pub const NOTIFICATION: i32 = -2;

/// A menu entry (id 0 is the root).
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub id: i32,
    pub label: String,
    pub enabled: bool,
    pub separator: bool,
    pub checked: Option<bool>,
    pub children: Vec<Item>,
}

impl Item {
    pub fn action(id: i32, label: &str) -> Item {
        Item { id, label: label.to_string(), enabled: true, separator: false, checked: None, children: Vec::new() }
    }

    pub fn disabled(id: i32, label: &str) -> Item {
        Item { enabled: false, ..Item::action(id, label) }
    }

    pub fn check(id: i32, label: &str, checked: bool) -> Item {
        Item { checked: Some(checked), ..Item::action(id, label) }
    }

    pub fn separator(id: i32) -> Item {
        Item { separator: true, ..Item::action(id, "") }
    }

    pub fn submenu(id: i32, label: &str, children: Vec<Item>) -> Item {
        Item { children, ..Item::action(id, label) }
    }

    fn find(&self, id: i32) -> Option<&Item> {
        if self.id == id {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.find(id))
    }
}

struct State {
    tooltip: String,
    color: (u8, u8, u8),
    root: Item,
    revision: u32,
    notification: u32,
}

pub struct Tray {
    bus: Arc<Bus>,
    state: Arc<Mutex<State>>,
    /// A panel shows the icon (a StatusNotifierWatcher took it).
    pub registered: bool,
}

/// dbusmenu labels use "_" for keyboard shortcuts: a real underscore is doubled.
fn label(s: &str) -> String {
    s.replace('_', "__")
}

fn item_props(it: &Item) -> V {
    let mut p = Vec::new();
    if it.separator {
        p.push(("type", V::s("separator")));
    } else {
        p.push(("label", V::S(label(&it.label))));
        if !it.enabled {
            p.push(("enabled", V::B(false)));
        }
        if let Some(c) = it.checked {
            p.push(("toggle-type", V::s("checkmark")));
            p.push(("toggle-state", V::I(c as i32)));
        }
        if !it.children.is_empty() || it.id == 0 {
            p.push(("children-display", V::s("submenu")));
        }
    }
    V::dict(p)
}

/// (id, properties, children) down to `depth` levels (-1 = all).
fn layout(it: &Item, depth: i32) -> V {
    let children = if depth == 0 { Vec::new() } else { it.children.iter().map(|c| V::var(layout(c, depth - 1))).collect() };
    V::R(vec![V::I(it.id), item_props(it), V::A("v".into(), children)])
}

/// The icon in a few sizes, ARGB in network byte order.
fn pixmaps(rgb: (u8, u8, u8)) -> V {
    let icons = [16u32, 22, 24, 32, 48]
        .iter()
        .map(|&size| {
            let mut bytes = Vec::with_capacity((size * size * 4) as usize);
            for a in crate::icon::coverage(size) {
                bytes.extend([V::Y(a), V::Y(rgb.0), V::Y(rgb.1), V::Y(rgb.2)]);
            }
            V::R(vec![V::I(size as i32), V::I(size as i32), V::A("y".into(), bytes)])
        })
        .collect();
    V::A("(iiay)".into(), icons)
}

/// The icon for notifications (`image-data`: 48×48 RGBA), so they have it also when the
/// icon is not installed in the icon theme (tar.gz).
fn image_data() -> V {
    const SIZE: u32 = 48;
    let mut bytes = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for a in crate::icon::coverage(SIZE) {
        bytes.extend([V::Y(0x3B), V::Y(0x82), V::Y(0xF6), V::Y(a)]);
    }
    V::R(vec![V::I(SIZE as i32), V::I(SIZE as i32), V::I(SIZE as i32 * 4), V::B(true), V::I(8), V::I(4), V::A("y".into(), bytes)])
}

fn sni_props(st: &State) -> Vec<(&'static str, V)> {
    let none = || V::A("(iiay)".into(), Vec::new());
    vec![
        ("Category", V::s("ApplicationStatus")),
        ("Id", V::s("frigate-pip")),
        ("Title", V::s("FrigatePiP")),
        ("Status", V::s("Active")),
        ("WindowId", V::I(0)),
        ("IconName", V::s("")),
        ("IconPixmap", pixmaps(st.color)),
        ("OverlayIconName", V::s("")),
        ("OverlayIconPixmap", none()),
        ("AttentionIconName", V::s("")),
        ("AttentionIconPixmap", none()),
        ("AttentionMovieName", V::s("")),
        ("ToolTip", V::R(vec![V::s(""), none(), V::s("FrigatePiP"), V::S(st.tooltip.clone())])),
        ("ItemIsMenu", V::B(false)),
        ("Menu", V::O(MENU_PATH.into())),
        ("IconThemePath", V::s("")),
    ]
}

fn menu_props() -> Vec<(&'static str, V)> {
    vec![("Version", V::U(3)), ("TextDirection", V::s("ltr")), ("Status", V::s("normal")), ("IconThemePath", V::A("s".into(), Vec::new()))]
}

fn introspect(path: &str) -> String {
    let iface = match path {
        ITEM_PATH => {
            r#"<interface name="org.kde.StatusNotifierItem"><method name="Activate"><arg type="i" direction="in"/><arg type="i" direction="in"/></method><method name="SecondaryActivate"><arg type="i" direction="in"/><arg type="i" direction="in"/></method><method name="ContextMenu"><arg type="i" direction="in"/><arg type="i" direction="in"/></method><method name="Scroll"><arg type="i" direction="in"/><arg type="s" direction="in"/></method><signal name="NewIcon"/><signal name="NewToolTip"/><signal name="NewStatus"><arg type="s"/></signal><property name="Id" type="s" access="read"/><property name="Category" type="s" access="read"/><property name="Title" type="s" access="read"/><property name="Status" type="s" access="read"/><property name="IconPixmap" type="a(iiay)" access="read"/><property name="ToolTip" type="(sa(iiay)ss)" access="read"/><property name="ItemIsMenu" type="b" access="read"/><property name="Menu" type="o" access="read"/></interface>"#
        }
        MENU_PATH => {
            r#"<interface name="com.canonical.dbusmenu"><method name="GetLayout"><arg type="i" direction="in"/><arg type="i" direction="in"/><arg type="as" direction="in"/><arg type="u" direction="out"/><arg type="(ia{sv}av)" direction="out"/></method><method name="GetGroupProperties"><arg type="ai" direction="in"/><arg type="as" direction="in"/><arg type="a(ia{sv})" direction="out"/></method><method name="GetProperty"><arg type="i" direction="in"/><arg type="s" direction="in"/><arg type="v" direction="out"/></method><method name="Event"><arg type="i" direction="in"/><arg type="s" direction="in"/><arg type="v" direction="in"/><arg type="u" direction="in"/></method><method name="EventGroup"><arg type="a(isvu)" direction="in"/><arg type="ai" direction="out"/></method><method name="AboutToShow"><arg type="i" direction="in"/><arg type="b" direction="out"/></method><method name="AboutToShowGroup"><arg type="ai" direction="in"/><arg type="ai" direction="out"/><arg type="ai" direction="out"/></method><signal name="LayoutUpdated"><arg type="u"/><arg type="i"/></signal><property name="Version" type="u" access="read"/><property name="Status" type="s" access="read"/></interface>"#
        }
        _ => "",
    };
    format!(
        r#"<!DOCTYPE node PUBLIC "-//freedesktop//DTD D-BUS Object Introspection 1.0//EN" "http://www.freedesktop.org/standards/dbus/1.0/introspect.dtd"><node>{iface}<interface name="org.freedesktop.DBus.Properties"><method name="Get"><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="v" direction="out"/></method><method name="GetAll"><arg type="s" direction="in"/><arg type="a{{sv}}" direction="out"/></method></interface><interface name="org.freedesktop.DBus.Introspectable"><method name="Introspect"><arg type="s" direction="out"/></method></interface></node>"#
    )
}

fn arg_str(m: &Msg, i: usize) -> &str {
    m.body.get(i).and_then(V::as_str).unwrap_or("")
}

fn arg_int(m: &Msg, i: usize) -> i64 {
    m.body.get(i).and_then(V::as_i64).unwrap_or(0)
}

/// Method calls to our objects and the signals we listen to. `clicks` are collected and
/// handed over after the state lock is released.
fn handle(bus: &Bus, m: &Msg, state: &Mutex<State>, service: &str, clicks: &mut Vec<i32>) {
    if m.kind == SIGNAL {
        if m.is("org.freedesktop.DBus", "NameOwnerChanged") && arg_str(m, 0) == WATCHER && !arg_str(m, 2).is_empty() {
            // The panel (re)started: show the icon there again.
            bus.call_nowait(WATCHER, "/StatusNotifierWatcher", WATCHER, "RegisterStatusNotifierItem", vec![V::s(service)]);
        } else if m.is(NOTIFICATIONS, "ActionInvoked") && arg_int(m, 0) as u32 == state.lock().unwrap().notification {
            clicks.push(NOTIFICATION);
        }
        return;
    }
    if m.kind != CALL {
        return;
    }
    let (path, iface, member) = (m.path.as_deref().unwrap_or(""), m.interface.as_deref().unwrap_or(""), m.member.as_deref().unwrap_or(""));
    let st = state.lock().unwrap();
    let props = |path: &str| match path {
        ITEM_PATH => Some(sni_props(&st)),
        MENU_PATH => Some(menu_props()),
        _ => None,
    };
    match (path, iface, member) {
        (_, "org.freedesktop.DBus.Peer", "Ping") => bus.reply(m, Vec::new()),
        (ITEM_PATH | MENU_PATH, "org.freedesktop.DBus.Introspectable", "Introspect") => bus.reply(m, vec![V::S(introspect(path))]),
        (ITEM_PATH | MENU_PATH, PROPS, "Get") => match props(path).and_then(|p| p.into_iter().find(|(k, _)| *k == arg_str(m, 1))) {
            Some((_, v)) => bus.reply(m, vec![V::var(v)]),
            None => bus.error(m, "org.freedesktop.DBus.Error.UnknownProperty", "unknown property"),
        },
        (ITEM_PATH | MENU_PATH, PROPS, "GetAll") => bus.reply(m, vec![V::dict(props(path).unwrap_or_default())]),
        (ITEM_PATH, ITEM_IFACE, "Activate") => {
            clicks.push(ACTIVATE);
            bus.reply(m, Vec::new());
        }
        (ITEM_PATH, ITEM_IFACE, _) => bus.reply(m, Vec::new()),
        (MENU_PATH, MENU_IFACE, "GetLayout") => {
            let parent = st.root.find(arg_int(m, 0) as i32).unwrap_or(&st.root);
            bus.reply(m, vec![V::U(st.revision), layout(parent, arg_int(m, 1) as i32)]);
        }
        (MENU_PATH, MENU_IFACE, "GetGroupProperties") => {
            let ids: Vec<i32> = match m.body.first() {
                Some(V::A(_, ids)) => ids.iter().filter_map(V::as_i64).map(|i| i as i32).collect(),
                _ => Vec::new(),
            };
            let items = ids.iter().filter_map(|&id| st.root.find(id)).map(|it| V::R(vec![V::I(it.id), item_props(it)])).collect();
            bus.reply(m, vec![V::A("(ia{sv})".into(), items)]);
        }
        (MENU_PATH, MENU_IFACE, "GetProperty") => {
            let found = st.root.find(arg_int(m, 0) as i32).and_then(|it| match item_props(it) {
                V::A(_, entries) => entries.into_iter().find_map(|e| match e {
                    V::E(k, v) if k.as_str() == Some(arg_str(m, 1)) => Some(*v),
                    _ => None,
                }),
                _ => None,
            });
            match found {
                Some(v) => bus.reply(m, vec![v]),
                None => bus.error(m, "org.freedesktop.DBus.Error.InvalidArgs", "unknown property"),
            }
        }
        (MENU_PATH, MENU_IFACE, "Event") => {
            if arg_str(m, 1) == "clicked" {
                clicks.push(arg_int(m, 0) as i32);
            }
            bus.reply(m, Vec::new());
        }
        (MENU_PATH, MENU_IFACE, "EventGroup") => {
            if let Some(V::A(_, events)) = m.body.first() {
                for e in events {
                    if let V::R(f) = e {
                        if f.get(1).and_then(V::as_str) == Some("clicked") {
                            clicks.extend(f.first().and_then(V::as_i64).map(|i| i as i32));
                        }
                    }
                }
            }
            bus.reply(m, vec![V::A("i".into(), Vec::new())]);
        }
        (MENU_PATH, MENU_IFACE, "AboutToShow") => bus.reply(m, vec![V::B(false)]),
        (MENU_PATH, MENU_IFACE, "AboutToShowGroup") => bus.reply(m, vec![V::A("i".into(), Vec::new()), V::A("i".into(), Vec::new())]),
        _ => bus.error(m, "org.freedesktop.DBus.Error.UnknownMethod", &format!("{iface}.{member} is not supported")),
    }
}

impl Tray {
    /// Puts the icon into the panel; `on_click` gets the menu item ids (also `ACTIVATE`,
    /// `NOTIFICATION`). Fails when there is no session bus.
    pub fn start(on_click: impl Fn(i32) + Send + 'static) -> Result<Tray, String> {
        let service = format!("org.kde.StatusNotifierItem-{}-1", std::process::id());
        let state = Arc::new(Mutex::new(State {
            tooltip: String::new(),
            color: (0x94, 0x94, 0x94),
            root: Item::submenu(0, "", Vec::new()),
            revision: 1,
            notification: 0,
        }));
        let bus = {
            let (state, service) = (state.clone(), service.clone());
            Bus::session(move |bus, m| {
                let mut clicks = Vec::new();
                handle(bus, &m, &state, &service, &mut clicks);
                for id in clicks {
                    on_click(id);
                }
            })?
        };
        bus.bus_call("RequestName", vec![V::S(service.clone()), V::U(4)])?;
        for rule in [
            "type='signal',sender='org.freedesktop.DBus',interface='org.freedesktop.DBus',member='NameOwnerChanged',arg0='org.kde.StatusNotifierWatcher'",
            "type='signal',interface='org.freedesktop.Notifications',member='ActionInvoked'",
        ] {
            let _ = bus.bus_call("AddMatch", vec![V::s(rule)]);
        }
        let registered = match bus.call(WATCHER, "/StatusNotifierWatcher", WATCHER, "RegisterStatusNotifierItem", vec![V::S(service.clone())]) {
            Ok(_) => true,
            Err(e) => {
                crate::log!("No tray on this desktop ({e})");
                false
            }
        };
        crate::log!("Tray icon {service}{}", if registered { "" } else { " (waiting for a tray)" });
        Ok(Tray { bus, state, registered })
    }

    /// New state: the tooltip and status, the icon colour and the menu.
    pub fn update(&self, tooltip: &str, color: (u8, u8, u8), menu: Vec<Item>) {
        let (mut icon, mut tip, mut layout) = (false, false, false);
        {
            let mut st = self.state.lock().unwrap();
            if st.color != color {
                st.color = color;
                icon = true;
            }
            if st.tooltip != tooltip {
                st.tooltip = tooltip.to_string();
                tip = true;
            }
            if st.root.children != menu {
                st.root.children = menu;
                st.revision += 1;
                layout = true;
            }
        }
        if icon {
            self.bus.signal(ITEM_PATH, ITEM_IFACE, "NewIcon", Vec::new());
        }
        if tip {
            self.bus.signal(ITEM_PATH, ITEM_IFACE, "NewToolTip", Vec::new());
        }
        if layout {
            let revision = self.state.lock().unwrap().revision;
            self.bus.signal(MENU_PATH, MENU_IFACE, "LayoutUpdated", vec![V::U(revision), V::I(0)]);
        }
    }

    /// A desktop notification; with `action`, clicking it (or that button) opens the settings.
    /// The click is the "default" action without a label: servers that draw every action
    /// as a button (XFCE) leave unlabelled ones out, so the button is not there twice.
    pub fn notify(&self, summary: &str, body: &str, action: Option<&str>) {
        let actions = action.map(|a| vec![V::s("default"), V::s(""), V::s("settings"), V::s(a)]).unwrap_or_default();
        let args = vec![
            V::s("FrigatePiP"),
            V::U(0),
            V::s("frigate-pip"),
            V::s(summary),
            V::s(body),
            V::A("s".into(), actions),
            V::dict(vec![("desktop-entry", V::s("frigate-pip")), ("image-data", image_data())]),
            V::I(-1),
        ];
        match self.bus.call(NOTIFICATIONS, "/org/freedesktop/Notifications", NOTIFICATIONS, "Notify", args) {
            Ok(r) => self.state.lock().unwrap().notification = r.first().and_then(V::as_i64).unwrap_or(0) as u32,
            Err(e) => crate::log!("Desktop notification failed: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_layout() {
        let root = Item::submenu(
            0,
            "",
            vec![
                Item::disabled(1, "Connected"),
                Item::separator(2),
                Item::submenu(4, "Show camera", vec![Item::action(100, "front_door")]),
                Item::check(53, "Until resumed", true),
            ],
        );
        let V::R(top) = layout(&root, -1) else { panic!() };
        assert_eq!(top[0], V::I(0));
        let V::A(_, children) = &top[2] else { panic!() };
        assert_eq!(children.len(), 4);
        assert_eq!(layout(&root, -1).signature(), "(ia{sv}av)");
        // Depth 1: the submenu is listed without its items.
        let V::R(top) = layout(&root, 1) else { panic!() };
        let V::A(_, children) = &top[2] else { panic!() };
        let V::Var(sub) = &children[2] else { panic!() };
        let V::R(sub) = &**sub else { panic!() };
        assert_eq!(sub[2], V::A("v".into(), Vec::new()));
        // Underscores survive (dbusmenu treats a single one as a shortcut marker).
        let V::A(_, props) = item_props(&Item::action(100, "front_door")) else { panic!() };
        assert!(props.contains(&V::E(Box::new(V::s("label")), Box::new(V::var(V::s("front__door"))))));
        assert_eq!(root.find(100).map(|i| i.label.as_str()), Some("front_door"));
    }

    #[test]
    fn icon_pixmaps() {
        let V::A(sig, icons) = pixmaps((0x3B, 0x82, 0xF6)) else { panic!() };
        assert_eq!(sig, "(iiay)");
        let V::R(first) = &icons[0] else { panic!() };
        let V::A(_, bytes) = &first[2] else { panic!() };
        assert_eq!((first[0].clone(), bytes.len()), (V::I(16), 16 * 16 * 4));
        assert_eq!(image_data().signature(), "(iiibiiay)");
    }
}
