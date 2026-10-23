//! What the IBus daemon talks to, and the values it is told in: the factory it asks for
//! the engine, the engine itself, and the component, the description and the text that
//! the wire form of IBus asks for (`OBS-219`, measured against libibus 1.5.34).
//!
//! The engine does nothing but exist. Every key the person types while it is the global
//! engine is handed back unconsumed, so the keyboard works as it did, and the one thing
//! it ever does is the commit that `mod.rs` emits from its path.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::Sender;

use zbus::zvariant::{OwnedObjectPath, StructureBuilder, Value};
use zbus::{ObjectServer, interface};

use super::Failed;
use crate::linux_ibus_rules::{is_plain_escape, is_release};

pub(super) const BUS: &str = "org.freedesktop.IBus";
pub(super) const BUS_PATH: &str = "/org/freedesktop/IBus";
pub(super) const FACTORY_PATH: &str = "/org/freedesktop/IBus/Factory";
pub(super) const ENGINE_INTERFACE: &str = "org.freedesktop.IBus.Engine";

/// What the daemon tells the engine that the send waits for.
#[derive(Debug)]
pub(super) enum Event {
    /// The daemon asked for the engine and it was made, at this path.
    Created(String),
    /// An input context took the focus, so a commit has somewhere to go. It also comes
    /// when the focus is on the desktop and not in a field (`OBS-219`).
    FocusIn,
}

/// The object the daemon asks to make the engine, once it has been told to make ours the
/// global one.
pub(super) struct Factory {
    name: String,
    events: Sender<Event>,
    escape: Arc<AtomicBool>,
    made: AtomicU32,
}

impl Factory {
    /// `escape` is set when the person presses `Escape` while the engine is the global one.
    pub(super) fn new(name: String, events: Sender<Event>, escape: Arc<AtomicBool>) -> Self {
        Self {
            name,
            events,
            escape,
            made: AtomicU32::new(0),
        }
    }
}

#[interface(name = "org.freedesktop.IBus.Factory")]
impl Factory {
    async fn create_engine(
        &self,
        name: String,
        #[zbus(object_server)] server: &ObjectServer,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        if name != self.name {
            return Err(zbus::fdo::Error::Failed(
                "this factory makes the engine of one send".to_owned(),
            ));
        }
        let number = self.made.fetch_add(1, Ordering::Relaxed) + 1;
        let path = format!("{BUS_PATH}/Engine/{number}");
        let engine = Engine {
            events: self.events.clone(),
            escape: Arc::clone(&self.escape),
        };
        server.at(path.as_str(), engine).await?;
        server.at(path.as_str(), Service).await?;
        let _ = self.events.send(Event::Created(path.clone()));
        OwnedObjectPath::try_from(path).map_err(|error| zbus::fdo::Error::Failed(error.to_string()))
    }
}

struct Engine {
    events: Sender<Event>,
    escape: Arc<AtomicBool>,
}

// The daemon calls these on every engine. The focus matters here, and `Escape`, which is
// the person asking for the send to stop. The rest is answered so that the daemon logs no
// warnings. Every other key is handed back unconsumed, so the keyboard works as it did.
#[interface(name = "org.freedesktop.IBus.Engine")]
impl Engine {
    fn process_key_event(&self, keyval: u32, _keycode: u32, state: u32) -> bool {
        if !is_plain_escape(keyval, state) {
            return false;
        }
        // Taken whole, pressed and let go, so that the application does not get half of it
        // (`D96`). It is the person's again once this send is over, because so is the engine.
        if !is_release(state) {
            self.escape.store(true, Ordering::SeqCst);
        }
        true
    }
    fn set_cursor_location(&self, _x: i32, _y: i32, _width: i32, _height: i32) {}
    fn set_capabilities(&self, _capabilities: u32) {}
    fn property_activate(&self, _name: String, _state: u32) {}
    fn property_show(&self, _name: String) {}
    fn property_hide(&self, _name: String) {}
    fn candidate_clicked(&self, _index: u32, _button: u32, _state: u32) {}
    fn set_surrounding_text(&self, _text: Value<'_>, _cursor: u32, _anchor: u32) {}
    fn focus_in(&self) {
        let _ = self.events.send(Event::FocusIn);
    }
    fn focus_in_id(&self, _object_path: String, _client: String) {
        let _ = self.events.send(Event::FocusIn);
    }
    fn focus_out(&self) {}
    fn focus_out_id(&self, _object_path: String) {}
    fn reset(&self) {}
    fn enable(&self) {}
    fn disable(&self) {}
    fn page_up(&self) {}
    fn page_down(&self) {}
    fn cursor_up(&self) {}
    fn cursor_down(&self) {}
}

struct Service;

#[interface(name = "org.freedesktop.IBus.Service")]
impl Service {
    fn destroy(&self) {}
}

/// An `IBusText` without attributes, as the variant `CommitText` carries: the name of the
/// type, an empty table of properties, the text, and an `IBusAttrList` of the same shape
/// with nothing in it.
pub(super) fn ibus_text(text: &str) -> Result<Value<'static>, Failed> {
    let attributes = StructureBuilder::new()
        .add_field("IBusAttrList".to_owned())
        .add_field(HashMap::<String, Value<'static>>::new())
        .add_field(Vec::<Value<'static>>::new())
        .build()?;
    let text = StructureBuilder::new()
        .add_field("IBusText".to_owned())
        .add_field(HashMap::<String, Value<'static>>::new())
        .add_field(text.to_owned())
        .add_field(Value::Value(Box::new(Value::Structure(attributes))))
        .build()?;
    Ok(Value::Structure(text))
}

/// The description of the engine: its name, long name, description, language, licence,
/// author, icon and layout, then the rank and eight empty strings the type has room for.
/// The layout is the one that means "leave the layout as it is".
fn engine_description(name: &str) -> Result<Value<'static>, Failed> {
    let mut description = StructureBuilder::new()
        .add_field("IBusEngineDesc".to_owned())
        .add_field(HashMap::<String, Value<'static>>::new());
    for field in [
        name,
        "Naughty Keyboard",
        "Types the value of one send",
        "en",
        "GPL",
        "nkb",
        "",
        "default",
    ] {
        description = description.add_field(field.to_owned());
    }
    description = description.add_field(0u32);
    for _ in 0..8 {
        description = description.add_field(String::new());
    }
    Ok(Value::Structure(description.build()?))
}

/// The component the engine belongs to, which the daemon is told of with
/// `RegisterComponent` and forgets with the connection.
pub(super) fn component(name: &str) -> Result<Value<'static>, Failed> {
    let mut component = StructureBuilder::new()
        .add_field("IBusComponent".to_owned())
        .add_field(HashMap::<String, Value<'static>>::new());
    for field in [
        format!("org.nkb.{name}"),
        "Naughty Keyboard".to_owned(),
        "1.0".to_owned(),
        "GPL".to_owned(),
        "nkb".to_owned(),
        String::new(),
        String::new(),
        name.to_owned(),
    ] {
        component = component.add_field(field);
    }
    component = component.add_field(Vec::<Value<'static>>::new());
    component = component.add_field(vec![Value::Value(Box::new(engine_description(name)?))]);
    Ok(Value::Structure(component.build()?))
}

/// The name inside the `GlobalEngine` property: a variant holding an `IBusEngineDesc`
/// structure whose third field is the name.
pub(super) fn name_of_engine(value: &Value<'_>) -> Option<String> {
    let Value::Value(inner) = value else {
        return None;
    };
    let Value::Structure(structure) = &**inner else {
        return None;
    };
    match structure.fields().get(2) {
        Some(Value::Str(name)) => Some(name.to_string()),
        _ => None,
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;

    fn signature(value: &Value<'_>) -> String {
        value.value_signature().to_string()
    }

    #[test]
    fn the_values_have_the_shape_libibus_reads() {
        // Measured against libibus 1.5.34 (`OBS-219`): a daemon that is told a value of
        // another shape drops the call or the signal, and says nothing.
        assert_eq!(signature(&ibus_text("x").expect("a text")), "(sa{sv}sv)");
        assert_eq!(
            signature(&component("nkb-send-1").expect("a component")),
            "(sa{sv}ssssssssavav)"
        );
        assert_eq!(
            signature(&engine_description("nkb-send-1").expect("a description")),
            "(sa{sv}ssssssssussssssss)"
        );
    }

    #[test]
    fn the_name_of_an_engine_is_read_from_the_global_engine_property() {
        let value = Value::Value(Box::new(
            engine_description("xkb:pl::pol").expect("a description"),
        ));
        assert_eq!(name_of_engine(&value).as_deref(), Some("xkb:pl::pol"));
        assert_eq!(name_of_engine(&Value::from("x")), None);
        assert_eq!(
            name_of_engine(&Value::Value(Box::new(Value::from("x")))),
            None
        );
    }
}
