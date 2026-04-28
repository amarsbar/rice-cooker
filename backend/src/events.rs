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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Step {
    Preflight,
    Evict,
    Clone,
    Deps,
    Symlink,
    Record,
    Notifiers,
    KillQuickshell,
    Launch,
    Verify,
    Replay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepState {
    Start,
    Done,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_variant_roundtrips_through_ndjson_schema() {
        let cases: &[(Event, &str)] = &[
            (
                Event::Hello {
                    version: 1,
                    subcommand: "install".into(),
                },
                r#"{"type":"hello","version":1,"subcommand":"install"}"#,
            ),
            // Pin every Step wire form.
            (
                Event::Step {
                    step: Step::Preflight,
                    state: StepState::Start,
                },
                r#"{"type":"step","step":"preflight","state":"start"}"#,
            ),
            (
                Event::Step {
                    step: Step::Evict,
                    state: StepState::Done,
                },
                r#"{"type":"step","step":"evict","state":"done"}"#,
            ),
            (
                Event::Step {
                    step: Step::Clone,
                    state: StepState::Start,
                },
                r#"{"type":"step","step":"clone","state":"start"}"#,
            ),
            (
                Event::Step {
                    step: Step::Deps,
                    state: StepState::Done,
                },
                r#"{"type":"step","step":"deps","state":"done"}"#,
            ),
            (
                Event::Step {
                    step: Step::Symlink,
                    state: StepState::Start,
                },
                r#"{"type":"step","step":"symlink","state":"start"}"#,
            ),
            (
                Event::Step {
                    step: Step::Record,
                    state: StepState::Done,
                },
                r#"{"type":"step","step":"record","state":"done"}"#,
            ),
            (
                Event::Step {
                    step: Step::Notifiers,
                    state: StepState::Start,
                },
                r#"{"type":"step","step":"notifiers","state":"start"}"#,
            ),
            (
                Event::Step {
                    step: Step::KillQuickshell,
                    state: StepState::Done,
                },
                r#"{"type":"step","step":"kill_quickshell","state":"done"}"#,
            ),
            (
                Event::Step {
                    step: Step::Launch,
                    state: StepState::Start,
                },
                r#"{"type":"step","step":"launch","state":"start"}"#,
            ),
            (
                Event::Step {
                    step: Step::Verify,
                    state: StepState::Done,
                },
                r#"{"type":"step","step":"verify","state":"done"}"#,
            ),
            (
                Event::Step {
                    step: Step::Replay,
                    state: StepState::Start,
                },
                r#"{"type":"step","step":"replay","state":"start"}"#,
            ),
            // Success omits None on serialize and reconstructs it via #[serde(default)].
            (
                Event::Success {
                    active: Some("x".into()),
                },
                r#"{"type":"success","active":"x"}"#,
            ),
            (
                Event::Fail {
                    stage: "precheck".into(),
                    reason: "missing_plugins".into(),
                    plugins: Some(vec!["Foo".into()]),
                    log_tail: None,
                },
                r#"{"type":"fail","stage":"precheck","reason":"missing_plugins","plugins":["Foo"]}"#,
            ),
        ];
        for (ev, wire) in cases {
            let got = serde_json::to_string(ev).unwrap();
            assert_eq!(&got, wire, "serialize");
            let back: Event = serde_json::from_str(&got).unwrap();
            assert_eq!(&back, ev, "roundtrip");
        }
    }

    #[test]
    fn writer_emits_ndjson_line_per_event() {
        let mut buf = Vec::new();
        let mut w = EventWriter::new(&mut buf);
        w.emit(&Event::Hello {
            version: 1,
            subcommand: "install".into(),
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
            "{\"type\":\"hello\",\"version\":1,\"subcommand\":\"install\"}\n\
             {\"type\":\"step\",\"step\":\"clone\",\"state\":\"start\"}\n"
        );
    }
}
