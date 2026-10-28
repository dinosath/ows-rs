use dioxus::prelude::*;
use ows_workflow_editor::{ValidationState, WorkflowEditor, WorkflowExecution};

const STARTER_WORKFLOW: &str = r#"{
  "document": {
    "dsl": "1.0.0",
    "namespace": "examples",
    "name": "hello-playground",
    "version": "1.0.0"
  },
  "do": [
    {
      "hello": {
        "set": {
          "greeting": "Hello, Open Workflow Specification!"
        }
      }
    }
  ]
}"#;

fn main() {
    dioxus::launch(App);
}

fn validate_ows(source: &str) -> ValidationState {
    match ows_dsl::Workflow::from_json(source) {
        Ok(workflow) => {
            let errors = workflow
                .validate()
                .issues
                .into_iter()
                .map(|issue| match issue.instance {
                    Some(path) => format!("{path}: {}", issue.message),
                    None => issue.message,
                })
                .collect::<Vec<_>>();
            ValidationState {
                checking: false,
                valid: errors.is_empty(),
                errors,
            }
        }
        Err(error) => ValidationState {
            checking: false,
            valid: false,
            errors: vec![error.to_string()],
        },
    }
}

#[component]
fn App() -> Element {
    let mut definition = use_signal(|| STARTER_WORKFLOW.to_string());
    let mut validation = use_signal(|| validate_ows(STARTER_WORKFLOW));
    let mut notice = use_signal(|| None::<String>);

    rsx! {
        document::Link { rel: "icon", href: "data:," }
        div { class: "playground-shell",
            div { class: "editor-frame",
                    WorkflowEditor {
                        definition: definition(),
                        on_change: move |value: String| {
                            validation.set(validate_ows(&value));
                            if let Ok(document) = serde_json::from_str::<serde_json::Value>(&value) {
                                if document.get("document").is_some_and(serde_json::Value::is_object)
                                    && document.get("do").is_some_and(serde_json::Value::is_array)
                                {
                                    definition.set(value);
                                }
                            }
                            notice.set(None);
                        },
                        on_validate: move |value: String| {
                            validation.set(validate_ows(&value));
                        },
                        validation: validation(),
                        execution: WorkflowExecution::default(),
                        on_execute: move |_| {
                            notice.set(Some("Execution needs a workflow runtime. This playground currently provides the visual editor.".to_string()));
                        },
                        execution_history: vec![],
                        notice: notice(),
                        on_listen_database_event: move |_| {
                            notice.set(Some("Database events are not connected in this static playground.".to_string()));
                        },
                    }
            }
        }
    }
}
