//! What the IBus daemon talks to, and the values it is told in: the factory it asks for
//! the engine, the engine itself, and the component, the description and the text that
//! the wire form of IBus asks for (`OBS-219`, measured against libibus 1.5.34).
//!
//! The engine does nothing but exist. Every key the person types while it is the global
//! engine is handed back unconsumed, so the keyboard works as it did, and the one thing
//! it ever does is the commit that `mod.rs` emits from its path. Two keys are the exception:
//! `Escape`, which is the person asking for a send to stop, and, while the keys that clear a
//! field are being sent (`keys.rs`), the one key of the recipe that is expected next, which
//! the engine reports and takes when the hand holds a modifier (`OBS-229`). And it tells the
//! send what the daemon says of the focus, with the input context and its client, which the
//! daemon names to an engine that asks for them (`FocusId`, `D159`).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, PoisonError};

use zbus::zvariant::{OwnedObjectPath, StructureBuilder, Value};
use zbus::{ObjectServer, interface};

use super::Failed;
use crate::linux_clearing_rules::{Expected, Judgement, judge_key};
use crate::linux_ibus_rules::{Heard, is_plain_escape, is_release};

/// The key the engine is waiting for, set by the send and read by the engine on every key.
pub(super) type Gate = Arc<Mutex<Option<Expected>>>;

/// Sets what the engine waits for next. Nothing, outside the dialogue of the recipe.
pub(super) fn expect(gate: &Gate, expected: Option<Expected>) {
    *gate.lock().unwrap_or_else(PoisonError::into_inner) = expected;
}

pub(super) const BUS: &str = "org.freedesktop.IBus";
pub(super) const BUS_PATH: &str = "/org/freedesktop/IBus";
pub(super) const FACTORY_PATH: &str = "/org/freedesktop/IBus/Factory";
pub(super) const ENGINE_INTERFACE: &str = "org.freedesktop.IBus.Engine";

/// What the daemon tells the engine that the send waits for.
#[derive(Debug)]
pub(super) enum Event {
    /// The daemon asked for the engine and it was made, at this path.
    Created(String),
    /// What the daemon said of the focus. An input context taking it also comes when the
    /// focus is on the desktop and not in a field (`OBS-219`), and then its client is the
    /// daemon's own (`D159`).
    Focus(Heard),
    /// The key the send waited for arrived, and this is what the engine found on it. Said
    /// once per key, for its press.
    Key(Judgement),
}

/// The object the daemon asks to make the engine, once it has been told to make ours the
/// global one.
pub(super) struct Factory {
    name: String,
    events: Sender<Event>,
    escape: Arc<AtomicBool>,
    gate: Gate,
    made: AtomicU32,
}

impl Factory {
    /// `escape` is set when the person presses `Escape` while the engine is the global one,
    /// and `gate` holds the key of the recipe that is waited for, if one is.
    pub(super) fn new(
        name: String,
        events: Sender<Event>,
        escape: Arc<AtomicBool>,
        gate: Gate,
    ) -> Self {
        Self {
            name,
            events,
            escape,
            gate,
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
            gate: Arc::clone(&self.gate),
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
    gate: Gate,
}

impl Engine {
    fn heard(&self, heard: Heard) {
        let _ = self.events.send(Event::Focus(heard));
    }

    /// What the engine makes of the key the recipe waits for: told to the send for its press,
    /// and taken when the hand holds a modifier the recipe does not have. Every other key is
    /// handed back.
    fn recipe_key(&self, keyval: u32, state: u32) -> bool {
        let expected = *self.gate.lock().unwrap_or_else(PoisonError::into_inner);
        let judged = judge_key(keyval, state, expected);
        if judged != Judgement::Unrelated && !is_release(state) {
            let _ = self.events.send(Event::Key(judged));
        }
        judged.taken()
    }
}

// The daemon calls these on every engine. The focus matters here, and `Escape`, which is
// the person asking for the send to stop. The rest is answered so that the daemon logs no
// warnings. Every other key is handed back unconsumed, so the keyboard works as it did.
#[interface(name = "org.freedesktop.IBus.Engine")]
impl Engine {
    fn process_key_event(&self, keyval: u32, _keycode: u32, state: u32) -> bool {
        if !is_plain_escape(keyval, state) {
            return self.recipe_key(keyval, state);
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
    /// Asks the daemon to name the input context and its client when the focus moves
    /// (`FocusInId`): read once per engine, as a property (`bus/engineproxy.c`).
    #[zbus(property)]
    fn focus_id(&self) -> bool {
        true
    }
    /// The engine does not want the text around the cursor, so the daemon never sends it
    /// the content of a field (untouchable rule 17). The daemon also reads this one after
    /// `FocusId`, and only its answer makes it say the focus again with the input context
    /// named (`_get_active_surrounding_text_cb`): without it the first focus of a send, told
    /// before the daemon knew of `FocusId`, stays without a name (measured, `D159`).
    #[zbus(property)]
    fn active_surrounding_text(&self) -> bool {
        false
    }
    // Before the daemon has read the property it calls the plain kind, so both are heard.
    fn focus_in(&self) {
        self.heard(Heard::In {
            context: None,
            client: None,
        });
    }
    fn focus_in_id(&self, object_path: String, client: String) {
        self.heard(Heard::In {
            context: Some(object_path),
            client: Some(client),
        });
    }
    fn focus_out(&self) {
        self.heard(Heard::Out);
    }
    fn focus_out_id(&self, _object_path: String) {
        self.heard(Heard::Out);
    }
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

    fn engine_with_gate() -> (Engine, Gate, std::sync::mpsc::Receiver<Event>) {
        let (events, heard) = std::sync::mpsc::channel();
        let gate: Gate = Arc::new(Mutex::new(None));
        let engine = Engine {
            events,
            escape: Arc::new(AtomicBool::new(false)),
            gate: Arc::clone(&gate),
        };
        (engine, gate, heard)
    }

    const HOME: u32 = 0xff50;
    const CTRL: u32 = 1 << 2;
    const RELEASED: u32 = 1 << 30;

    #[test]
    fn outside_the_dialogue_of_the_recipe_the_engine_takes_nothing_and_says_nothing() {
        let (engine, _gate, heard) = engine_with_gate();
        assert!(!engine.process_key_event(HOME, 0, CTRL));
        assert!(!engine.process_key_event(HOME, 0, 0));
        assert!(heard.try_recv().is_err());
    }

    #[test]
    fn the_key_waited_for_is_told_to_the_send_and_taken_only_when_a_modifier_is_held() {
        let (engine, gate, heard) = engine_with_gate();
        expect(
            &gate,
            Some(Expected {
                keysym: HOME,
                shifted: false,
            }),
        );
        assert!(!engine.process_key_event(HOME, 0, 0), "clean goes through");
        assert!(matches!(heard.try_recv(), Ok(Event::Key(Judgement::Clean))));
        assert!(
            engine.process_key_event(HOME, 0, CTRL),
            "held is taken, the document is safe"
        );
        assert!(matches!(
            heard.try_recv(),
            Ok(Event::Key(Judgement::Held("Ctrl")))
        ));
    }

    #[test]
    fn the_release_of_a_key_is_judged_as_its_press_but_said_once() {
        let (engine, gate, heard) = engine_with_gate();
        expect(
            &gate,
            Some(Expected {
                keysym: HOME,
                shifted: false,
            }),
        );
        assert!(engine.process_key_event(HOME, 0, CTRL | RELEASED));
        assert!(!engine.process_key_event(HOME, 0, RELEASED));
        assert!(heard.try_recv().is_err(), "a release says nothing");
    }

    #[test]
    fn every_other_key_goes_to_the_application_and_escape_is_still_the_persons() {
        let (engine, gate, heard) = engine_with_gate();
        expect(
            &gate,
            Some(Expected {
                keysym: HOME,
                shifted: false,
            }),
        );
        assert!(!engine.process_key_event(0x61, 0, CTRL), "a letter");
        assert!(!engine.process_key_event(0xffe3, 0, 0), "a modifier");
        assert!(heard.try_recv().is_err());
        assert!(engine.process_key_event(0xff1b, 0, 0), "Escape is taken");
        assert!(engine.escape.load(Ordering::SeqCst));
    }

    #[test]
    fn a_gate_that_is_emptied_stops_the_judging() {
        let (engine, gate, heard) = engine_with_gate();
        expect(
            &gate,
            Some(Expected {
                keysym: HOME,
                shifted: false,
            }),
        );
        expect(&gate, None);
        assert!(!engine.process_key_event(HOME, 0, CTRL));
        assert!(heard.try_recv().is_err());
    }

    #[test]
    fn what_the_daemon_says_of_the_focus_reaches_the_send_with_the_context_it_names() {
        let (engine, _gate, heard) = engine_with_gate();
        engine.focus_in();
        engine.focus_in_id(
            "/org/freedesktop/IBus/InputContext_1".to_owned(),
            "fake".to_owned(),
        );
        engine.focus_out_id("/org/freedesktop/IBus/InputContext_1".to_owned());
        engine.focus_out();
        let said: Vec<Heard> = heard
            .try_iter()
            .filter_map(|event| match event {
                Event::Focus(focus) => Some(focus),
                _ => None,
            })
            .collect();
        assert_eq!(
            said,
            [
                Heard::In {
                    context: None,
                    client: None
                },
                Heard::In {
                    context: Some("/org/freedesktop/IBus/InputContext_1".to_owned()),
                    client: Some("fake".to_owned())
                },
                Heard::Out,
                Heard::Out
            ]
        );
        assert!(engine.focus_id(), "the daemon is asked to name the context");
        assert!(
            !engine.active_surrounding_text(),
            "the content of a field is never asked for"
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
