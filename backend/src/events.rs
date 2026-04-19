use std::io::{self, Write};

use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;

pub struct EventWriter<W: Write> {
    inner: W,
}

impl<W: Write> EventWriter<W> {
    pub fn new(inner: W) -> Self {
        Self { inner }
    }

    pub fn emit(&mut self, event: &Event) -> io::Result<()> {
        serde_json::to_writer(&mut self.inner, event)?;
        self.inner.write_all(b"\n")?;
        self.inner.flush()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    Hello {
        version: u32,
        subcommand: String,
    },
    Step {
        step: Step,
        state: StepState,
    },
    Success {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        active: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        previous: Option<String>,
        #[serde(default, skip_serializing_if = "is_false")]
        dry_run: bool,
    },
    Fail {
        stage: String,
        reason: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        plugins: Option<Vec<String>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        log_tail: Option<String>,
    },
}

fn is_false(b: &bool) -> bool {
    !*b
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Step {
    Preflight,
    Clone,
    Entry,
    Precheck,
    Notifiers,
    KillQuickshell,
    Launch,
    Verify,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepState {
    Start,
    Done,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_hello_event() {
        let ev = Event::Hello {
            version: 1,
            subcommand: "apply".into(),
        };
        let s = serde_json::to_string(&ev).unwrap();
        assert_eq!(s, r#"{"type":"hello","version":1,"subcommand":"apply"}"#);
    }

    #[test]
    fn serializes_step_event() {
        let ev = Event::Step {
            step: Step::Clone,
            state: StepState::Start,
        };
        let s = serde_json::to_string(&ev).unwrap();
        assert_eq!(s, r#"{"type":"step","step":"clone","state":"start"}"#);

        let ev = Event::Step {
            step: Step::Precheck,
            state: StepState::Done,
        };
        let s = serde_json::to_string(&ev).unwrap();
        assert_eq!(s, r#"{"type":"step","step":"precheck","state":"done"}"#);
    }

    #[test]
    fn success_roundtrips_and_skips_defaults() {
        // Covers both skip_serializing_if (None/false omitted) and the matching
        // #[serde(default)] on deserialize.
        let original = Event::Success {
            active: Some("x".into()),
            previous: None,
            dry_run: false,
        };
        let wire = serde_json::to_string(&original).unwrap();
        assert_eq!(wire, r#"{"type":"success","active":"x"}"#);
        let back: Event = serde_json::from_str(&wire).unwrap();
        assert_eq!(back, original);
    }

    #[test]
    fn fail_roundtrips_with_plugins() {
        let original = Event::Fail {
            stage: "precheck".into(),
            reason: "missing_plugins".into(),
            plugins: Some(vec!["Foo".into()]),
            log_tail: None,
        };
        let wire = serde_json::to_string(&original).unwrap();
        assert_eq!(
            wire,
            r#"{"type":"fail","stage":"precheck","reason":"missing_plugins","plugins":["Foo"]}"#
        );
        let back: Event = serde_json::from_str(&wire).unwrap();
        assert_eq!(back, original);
    }

    #[test]
    fn writer_emits_ndjson_line_per_event() {
        let mut buf = Vec::new();
        let mut w = EventWriter::new(&mut buf);
        w.emit(&Event::Hello {
            version: 1,
            subcommand: "apply".into(),
        })
        .unwrap();
        w.emit(&Event::Step {
            step: Step::Clone,
            state: StepState::Start,
        })
        .unwrap();

        let out = String::from_utf8(buf).unwrap();
        assert_eq!(
            out,
            "{\"type\":\"hello\",\"version\":1,\"subcommand\":\"apply\"}\n\
             {\"type\":\"step\",\"step\":\"clone\",\"state\":\"start\"}\n"
        );
    }
}
