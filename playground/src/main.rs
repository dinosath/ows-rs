use dioxus::prelude::*;
use futures_util::future::LocalBoxFuture;
use http::Method;
use ows_workflow_editor::{
    ValidationState, WorkflowEditor, WorkflowExample, WorkflowExecution, WorkflowExecutionRecord,
    WorkflowExecutionStep,
};
use serde_json::{Map, Value, json};

const STARTER_WORKFLOW: &str = r#"{
  "document": {
    "dsl": "1.0.3",
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

const WORKFLOW_EXAMPLES: &[(&str, &str, &str)] = &[
    (
        "call",
        "HTTP call",
        include_str!("../../examples/workflows/call.yaml"),
    ),
    (
        "do",
        "Do sequence",
        include_str!("../../examples/workflows/do.yaml"),
    ),
    (
        "emit",
        "Emit event",
        include_str!("../../examples/workflows/emit.yaml"),
    ),
    (
        "for",
        "For loop",
        include_str!("../../examples/workflows/for.yaml"),
    ),
    (
        "fork",
        "Fork branches",
        include_str!("../../examples/workflows/fork.yaml"),
    ),
    (
        "listen",
        "Listen for event",
        include_str!("../../examples/workflows/listen.yaml"),
    ),
    (
        "raise",
        "Raise error",
        include_str!("../../examples/workflows/raise.yaml"),
    ),
    (
        "run",
        "Run command",
        include_str!("../../examples/workflows/run.yaml"),
    ),
    (
        "set",
        "Set values",
        include_str!("../../examples/workflows/set.yaml"),
    ),
    (
        "switch",
        "Switch",
        include_str!("../../examples/workflows/switch.yaml"),
    ),
    (
        "try",
        "Try and catch",
        include_str!("../../examples/workflows/try.yaml"),
    ),
    (
        "wait",
        "Wait",
        include_str!("../../examples/workflows/wait.yaml"),
    ),
    (
        "run_workflow_json",
        "Greeting (JSON)",
        include_str!("../../examples/workflows/run_workflow.json"),
    ),
    (
        "run_workflow_yaml",
        "Greeting (YAML)",
        include_str!("../../examples/workflows/run_workflow.yaml"),
    ),
    (
        "download_novel",
        "Download novel",
        include_str!("../../download-novel.yaml"),
    ),
];

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

fn normalize_import(source: &str) -> Result<(String, ValidationState), String> {
    let workflow = ows_dsl::from_bytes(source.as_bytes()).map_err(|error| error.to_string())?;
    let definition = serde_json::to_string_pretty(&workflow)
        .map_err(|error| format!("Could not normalize workflow: {error}"))?;
    let validation = validate_ows(&definition);
    Ok((definition, validation))
}

fn now_ms() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs_f64() * 1000.0)
            .unwrap_or_default()
    }
}

fn elapsed_ms(started_at: f64) -> i64 {
    (now_ms() - started_at).max(0.0).min(i64::MAX as f64) as i64
}

async fn run_client_workflow(
    source: &str,
    input: Value,
) -> Result<(Value, Vec<WorkflowExecutionStep>), String> {
    let workflow: Value = serde_json::from_str(source).map_err(|error| error.to_string())?;
    let tasks = workflow["do"]
        .as_array()
        .ok_or_else(|| "Workflow must contain a top-level `do` array".to_string())?;
    let mut current = input;
    let mut variables = Map::new();
    let mut events = Vec::new();
    let mut steps = Vec::new();
    execute_tasks(tasks, &mut current, &mut variables, &mut events, &mut steps).await?;
    Ok((current, steps))
}

fn execute_tasks<'a>(
    tasks: &'a [Value],
    current: &'a mut Value,
    variables: &'a mut Map<String, Value>,
    events: &'a mut Vec<Value>,
    steps: &'a mut Vec<WorkflowExecutionStep>,
) -> LocalBoxFuture<'a, Result<(), String>> {
    Box::pin(async move {
        let mut index = 0;
        let mut task_count = 0;
        while index < tasks.len() {
            task_count += 1;
            if task_count > 10_000 {
                return Err("Workflow exceeded the 10,000 task safety limit".to_string());
            }
            let task = &tasks[index];
            let (name, kind, body) = task_parts(task)?;
            let step_index = steps.len();
            let started = now_ms();
            let input = current.clone();
            let input_json = serde_json::to_string_pretty(&input).ok();
            let mut task_variables = variables.clone();
            task_variables.insert("input".into(), input.clone());
            let mut switch_target = None;
            let mut output = match kind.as_str() {
                "set" => resolve_value(&body["set"], &input, &task_variables)?,
                "do" => {
                    let nested = body["do"]
                        .as_array()
                        .ok_or_else(|| format!("{name}: `do` must be an array"))?;
                    let mut nested_output = input.clone();
                    execute_tasks(nested, &mut nested_output, variables, events, steps).await?;
                    nested_output
                }
                "for" => {
                    let config = &body["for"];
                    let expression = config["in"]
                        .as_str()
                        .ok_or_else(|| format!("{name}: `for.in` must be an expression"))?;
                    let collection = evaluate_expression(expression, &input, &task_variables)?;
                    let items = collection
                        .as_array()
                        .cloned()
                        .ok_or_else(|| format!("{name}: `for.in` must evaluate to an array"))?;
                    let each = config["each"].as_str().unwrap_or("item").to_string();
                    let at = config["at"].as_str().unwrap_or("index").to_string();
                    let nested = body["do"]
                        .as_array()
                        .ok_or_else(|| format!("{name}: `for.do` must be an array"))?;
                    let mut loop_output = input.clone();
                    for (iteration, item) in items.into_iter().enumerate() {
                        variables.insert(each.clone(), item);
                        variables.insert(at.clone(), json!(iteration));
                        execute_tasks(nested, &mut loop_output, variables, events, steps).await?;
                    }
                    variables.remove(&each);
                    variables.remove(&at);
                    loop_output
                }
                "fork" => {
                    let branches = body["fork"]["branches"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default();
                    let mut results = Vec::new();
                    for branch in branches {
                        let mut branch_output = input.clone();
                        let mut branch_vars = task_variables.clone();
                        execute_tasks(
                            std::slice::from_ref(&branch),
                            &mut branch_output,
                            &mut branch_vars,
                            events,
                            steps,
                        )
                        .await?;
                        results.push(branch_output);
                    }
                    Value::Array(results)
                }
                "switch" => {
                    let cases = body["switch"]
                        .as_array()
                        .ok_or_else(|| format!("{name}: `switch` must be an array"))?;
                    let mut target = None;
                    for case in cases {
                        let Some((_, config)) =
                            case.as_object().and_then(|object| object.iter().next())
                        else {
                            continue;
                        };
                        let matches = match config.get("when").and_then(Value::as_str) {
                            Some(expression) => {
                                truthy(&evaluate_expression(expression, &input, &task_variables)?)
                            }
                            None => true,
                        };
                        if matches {
                            target = config
                                .get("then")
                                .and_then(Value::as_str)
                                .map(str::to_string);
                            break;
                        }
                    }
                    if let Some(target) = target {
                        switch_target = Some(target);
                    }
                    input.clone()
                }
                "call" => {
                    if body["call"].as_str() != Some("http") {
                        return Err(format!(
                            "{name}: only the HTTP call target can run in the browser; `{}` needs a registered runtime function",
                            body["call"].as_str().unwrap_or("(missing)")
                        ));
                    }
                    call_http(&body["with"], &input, &task_variables)
                        .await
                        .map_err(|error| format!("{name}: {error}"))?
                }
                "emit" => {
                    let event = resolve_value(&body["emit"]["event"], &input, &task_variables)?;
                    let event = event.get("with").cloned().unwrap_or(event);
                    events.push(event.clone());
                    event
                }
                "listen" => {
                    if let Some(event) = events.pop() {
                        event
                            .get(body["listen"]["read"].as_str().unwrap_or("data"))
                            .cloned()
                            .unwrap_or(event)
                    } else {
                        Value::Null
                    }
                }
                "raise" => {
                    let error = &body["raise"]["error"];
                    return Err(format!(
                        "{name}: {}",
                        error["title"]
                            .as_str()
                            .or_else(|| error["detail"].as_str())
                            .unwrap_or("workflow raised an error")
                    ));
                }
                "run" => {
                    let shell = &body["run"]["shell"];
                    let command = shell["command"].as_str().unwrap_or_default();
                    if command != "echo" {
                        return Err(format!(
                            "{name}: browser execution supports only the `echo` command, not `{command}`"
                        ));
                    }
                    Value::String(
                        shell["arguments"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join(" "),
                    )
                }
                "wait" => Value::Null,
                "try" => {
                    let config = &body["try"];
                    let nested = config
                        .as_array()
                        .or_else(|| config["do"].as_array())
                        .ok_or_else(|| format!("{name}: `try` must contain a task array"))?;
                    let mut try_output = input.clone();
                    match execute_tasks(nested, &mut try_output, variables, events, steps).await {
                        Ok(()) => try_output,
                        Err(error) if body.get("catch").is_some() => {
                            let _ = error;
                            input.clone()
                        }
                        Err(error) => return Err(error),
                    }
                }
                other => return Err(format!("{name}: unsupported task type `{other}`")),
            };

            if let Some(expression) = body["output"]["as"].as_str() {
                task_variables.insert("input".into(), input.clone());
                task_variables.insert("output".into(), output.clone());
                output = evaluate_expression(expression, &input, &task_variables)?;
            }
            *current = output.clone();
            let step = make_step(
                name.clone(),
                "succeeded",
                started,
                input_json,
                Some(&output),
            );
            if kind == "do" {
                // A `do` scope starts before its child tasks. Insert its completed
                // snapshot ahead of the child logs so the log list follows the
                // workflow's execution hierarchy and source order.
                steps.insert(step_index, step);
            } else {
                steps.push(step);
            }

            if let Some(target) = switch_target {
                index = task_index(tasks, &target)
                    .ok_or_else(|| format!("{name}: switch target `{target}` was not found"))?;
                continue;
            }

            if let Some(target) = body.get("then").and_then(Value::as_str) {
                if target == "end" {
                    break;
                }
                if target != "continue" {
                    index = task_index(tasks, target)
                        .ok_or_else(|| format!("{name}: flow target `{target}` was not found"))?;
                    continue;
                }
            }
            index += 1;
        }
        Ok(())
    })
}

fn make_step(
    name: String,
    status: &str,
    started: f64,
    input: Option<String>,
    output: Option<&Value>,
) -> WorkflowExecutionStep {
    WorkflowExecutionStep {
        name,
        status: status.to_string(),
        duration_ms: Some(elapsed_ms(started)),
        input_json: input,
        output_json: output.and_then(|value| serde_json::to_string_pretty(value).ok()),
    }
}

fn task_index(tasks: &[Value], target: &str) -> Option<usize> {
    tasks.iter().position(|task| {
        task_parts(task)
            .ok()
            .is_some_and(|(name, _, _)| name == target)
    })
}

fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64().is_some_and(|number| number != 0.0),
        Value::String(value) => !value.is_empty(),
        _ => true,
    }
}

async fn call_http(
    args: &Value,
    input: &Value,
    variables: &Map<String, Value>,
) -> Result<Value, String> {
    let args = resolve_value(args, input, variables)?;
    let method = args["method"]
        .as_str()
        .unwrap_or("get")
        .to_ascii_uppercase();
    let method = Method::from_bytes(method.as_bytes())
        .map_err(|error| format!("invalid HTTP method: {error}"))?;
    let endpoint = args["endpoint"]
        .as_str()
        .or_else(|| args["endpoint"]["uri"].as_str())
        .ok_or("missing HTTP endpoint")?;
    let mut request = gloo_net::http::RequestBuilder::new(endpoint).method(method);
    if let Some(headers) = args["headers"].as_object() {
        for (key, value) in headers {
            // Browsers forbid setting User-Agent and ignore it for fetch requests.
            if key.eq_ignore_ascii_case("user-agent") {
                continue;
            }
            let value = value
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| value.to_string());
            request = request.header(key, &value);
        }
    }
    let response = if let Some(body) = args.get("body") {
        let body = body
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| body.to_string());
        request
            .body(body)
            .map_err(|error| error.to_string())?
            .send()
            .await
    } else {
        request.send().await
    }
    .map_err(|error| {
        format!("HTTP request failed; check the URL and the server's CORS policy: {error}")
    })?;
    let status = response.status();
    let content_type = response.headers().get("content-type").unwrap_or_default();
    let text = response
        .text()
        .await
        .map_err(|error| format!("could not read HTTP response: {error}"))?;
    if !(200..300).contains(&status) {
        return Err(format!("HTTP request returned status {status}: {text}"));
    }
    let content = if content_type.contains("json") {
        serde_json::from_str(&text).unwrap_or(Value::String(text))
    } else {
        Value::String(text)
    };
    if args["output"].as_str() == Some("response") {
        Ok(json!({ "statusCode": status, "content": content }))
    } else {
        Ok(content)
    }
}

fn task_parts<'a>(task: &'a Value) -> Result<(String, String, &'a Value), String> {
    let object = task
        .as_object()
        .ok_or_else(|| "task must be an object".to_string())?;
    if let Some(name) = object.get("name").and_then(Value::as_str) {
        let (kind, _) = find_task_kind(object)
            .ok_or_else(|| format!("{name}: could not determine task type"))?;
        return Ok((name.to_string(), kind.clone(), task));
    }
    let (name, body) = object
        .iter()
        .next()
        .ok_or_else(|| "task object is empty".to_string())?;
    let (kind, _) = body
        .as_object()
        .and_then(find_task_kind)
        .ok_or_else(|| format!("{name}: could not determine task type"))?;
    Ok((name.clone(), kind.clone(), body))
}

fn is_task_kind(key: &str) -> bool {
    matches!(
        key,
        "call"
            | "do"
            | "emit"
            | "for"
            | "fork"
            | "listen"
            | "raise"
            | "run"
            | "set"
            | "switch"
            | "try"
            | "wait"
    )
}

fn find_task_kind(fields: &Map<String, Value>) -> Option<(&String, &Value)> {
    fields
        .iter()
        .find(|(key, _)| key.as_str() != "do" && is_task_kind(key))
        .or_else(|| fields.get_key_value("do"))
}

fn resolve_value(
    value: &Value,
    input: &Value,
    variables: &Map<String, Value>,
) -> Result<Value, String> {
    match value {
        Value::String(source)
            if source.trim_start().starts_with("${") && source.trim_end().ends_with('}') =>
        {
            evaluate_expression(source, input, variables)
        }
        Value::Array(values) => values
            .iter()
            .map(|value| resolve_value(value, input, variables))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        Value::Object(fields) => fields
            .iter()
            .map(|(key, value)| {
                resolve_value(value, input, variables).map(|value| (key.clone(), value))
            })
            .collect::<Result<Map<String, Value>, _>>()
            .map(Value::Object),
        _ => Ok(value.clone()),
    }
}

fn evaluate_expression(
    source: &str,
    input: &Value,
    variables: &Map<String, Value>,
) -> Result<Value, String> {
    let source = source.trim();
    let expression_source = source
        .strip_prefix("${")
        .and_then(|source| source.strip_suffix('}'))
        .unwrap_or(source)
        .trim();
    let expression = ows_expressions::parse(expression_source)
        .map_err(|error| format!("invalid expression `{expression_source}`: {error}"))?;
    let mut variables = variables.clone();
    variables.insert("input".to_string(), input.clone());
    ows_expressions::eval_single(&expression, input, &ows_expressions::Env::new(variables))
        .map_err(|error| error.to_string())
}

#[component]
fn App() -> Element {
    let mut definition = use_signal(|| STARTER_WORKFLOW.to_string());
    let mut validation = use_signal(|| validate_ows(STARTER_WORKFLOW));
    let mut notice = use_signal(|| None::<String>);
    let mut execution = use_signal(WorkflowExecution::default);
    let execution_history = use_signal(Vec::<WorkflowExecutionRecord>::new);
    let mut run_count = use_signal(|| 0_u64);
    let mut import_open = use_signal(|| false);
    let mut import_url = use_signal(String::new);
    let mut import_error = use_signal(|| None::<String>);
    let mut importing = use_signal(|| false);
    let mut run_input = use_signal(|| "{}".to_string());
    let mut input_open = use_signal(|| false);
    let mut input_draft = use_signal(|| "{}".to_string());
    let mut input_error = use_signal(|| None::<String>);

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
                        on_import: move |_| {
                            import_url.set(String::new());
                            import_error.set(None);
                            import_open.set(true);
                        },
                        examples: WORKFLOW_EXAMPLES
                            .iter()
                            .map(|(id, label, _)| WorkflowExample { id: (*id).to_string(), label: (*label).to_string() })
                            .collect(),
                        on_select_example: move |id: String| {
                            if let Some((_, _, source)) = WORKFLOW_EXAMPLES.iter().find(|(example_id, _, _)| *example_id == id) {
                                match normalize_import(source) {
                                    Ok((source, state)) => {
                                        definition.set(source);
                                        validation.set(state);
                                        run_input.set("{}".to_string());
                                        notice.set(None);
                                    }
                                    Err(error) => notice.set(Some(format!("Could not load example: {error}"))),
                                }
                            }
                        },
                        on_configure_input: move |_| {
                            input_draft.set(run_input());
                            input_error.set(None);
                            input_open.set(true);
                        },
                        validation: validation(),
                        execution: execution(),
                        on_execute: move |_| {
                            if !validation().valid || validation().checking {
                                notice.set(Some("Fix validation issues before running the workflow.".to_string()));
                                return;
                            }
                            let started = now_ms();
                            let input = match serde_json::from_str::<Value>(&run_input()) {
                                Ok(input) => input,
                                Err(error) => {
                                    notice.set(Some(format!("Run input must be valid JSON: {error}")));
                                    return;
                                }
                            };
                            let started_at = js_sys::Date::new_0().to_iso_string().as_string().unwrap_or_default();
                            let id = format!("browser-{}", run_count() + 1);
                            run_count.set(run_count() + 1);
                            let source = definition();
                            let input_json = serde_json::to_string_pretty(&input).unwrap_or_else(|_| "{}".to_string());
                            execution.set(WorkflowExecution {
                                running: true,
                                execution_id: Some(id.clone()),
                                status: Some("running".to_string()),
                                started_at: Some(started_at.clone()),
                                duration_ms: None,
                                input_json: Some(input_json.clone()),
                                output: None,
                                error: None,
                                steps: Vec::new(),
                            });
                            let mut execution = execution;
                            let mut execution_history = execution_history;
                            let mut notice = notice;
                            spawn(async move {
                                let (status, output, error, steps) = match run_client_workflow(&source, input.clone()).await {
                                    Ok((output, steps)) => ("succeeded".to_string(), Some(output), None, steps),
                                    Err(error) => ("failed".to_string(), None, Some(error), Vec::new()),
                                };
                                let duration_ms = elapsed_ms(started);
                                let output_json = output.as_ref().and_then(|value| serde_json::to_string_pretty(value).ok());
                                execution.set(WorkflowExecution {
                                    running: false,
                                    execution_id: Some(id.clone()),
                                    status: Some(status.clone()),
                                    started_at: Some(started_at.clone()),
                                    duration_ms: Some(duration_ms),
                                    input_json: Some(input_json.clone()),
                                    output: output_json.clone(),
                                    error: error.clone(),
                                    steps: steps.clone(),
                                });
                                let record = WorkflowExecutionRecord {
                                    execution_id: id,
                                    status,
                                    started_at,
                                    duration_ms: Some(duration_ms),
                                    input_json,
                                    output_json,
                                    error: error.clone(),
                                    steps,
                                };
                                execution_history.with_mut(|records| records.insert(0, record));
                                notice.set(error);
                            });
                        },
                        execution_history: execution_history(),
                        notice: notice(),
                        on_listen_database_event: move |_| {
                            notice.set(Some("Database events are not connected in this static playground.".to_string()));
                        },
                    }
            }
            if import_open() {
                div { class: "workflow-import-overlay", role: "dialog", aria_modal: "true", aria_label: "Import workflow",
                    section { class: "workflow-import-modal",
                        header { class: "workflow-import-modal__header",
                            div {
                                p { class: "workflow-editor__eyebrow", "WORKFLOW" }
                                h2 { "Import workflow" }
                                p { "Load an OWS definition from your device or a public URL." }
                            }
                            button {
                                class: "workflow-editor__icon-action",
                                r#type: "button",
                                aria_label: "Close import dialog",
                                disabled: importing(),
                                onclick: move |_| import_open.set(false),
                                "×"
                            }
                        }
                        div { class: "workflow-import-modal__body",
                            label { class: "workflow-import-file",
                                span { class: "workflow-import-file__icon", "↑" }
                                strong { "Choose a workflow file" }
                                small { "JSON or YAML · stays on this device" }
                                input {
                                    r#type: "file",
                                    accept: ".json,.yaml,.yml,application/json,text/yaml,text/x-yaml",
                                    disabled: importing(),
                                    onchange: move |event| {
                                        let Some(file) = event.files().into_iter().next() else { return; };
                                        importing.set(true);
                                        import_error.set(None);
                                        let mut definition = definition;
                                        let mut validation = validation;
                                        let mut import_error = import_error;
                                        let mut import_open = import_open;
                                        let mut importing = importing;
                                        let mut notice = notice;
                                        spawn(async move {
                                            match file.read_string().await {
                                                Ok(source) => match normalize_import(&source) {
                                                    Ok((json, state)) => {
                                                        definition.set(json);
                                                        validation.set(state);
                                                        notice.set(None);
                                                        import_open.set(false);
                                                    }
                                                    Err(error) => import_error.set(Some(format!("Could not import {}: {error}", file.name()))),
                                                },
                                                Err(error) => import_error.set(Some(format!("Could not read {}: {error}", file.name()))),
                                            }
                                            importing.set(false);
                                        });
                                    },
                                }
                            }
                            div { class: "workflow-import-divider", "OR IMPORT FROM URL" }
                            label { class: "workflow-import-url",
                                span { "Workflow URL" }
                                input {
                                    r#type: "url",
                                    placeholder: "https://example.com/workflow.yaml",
                                    value: "{import_url}",
                                    disabled: importing(),
                                    oninput: move |event| import_url.set(event.value()),
                                }
                            }
                            p { class: "workflow-import-help", "The URL must allow browser cross-origin requests (CORS). The response is read in your browser and is not uploaded." }
                            if let Some(error) = import_error() {
                                p { class: "workflow-import-error", role: "alert", "{error}" }
                            }
                            div { class: "workflow-import-actions",
                                button {
                                    class: "button button-primary",
                                    r#type: "button",
                                    disabled: importing() || import_url().trim().is_empty(),
                                    onclick: move |_| {
                                        let url = import_url().trim().to_string();
                                        if url.is_empty() { return; }
                                        importing.set(true);
                                        import_error.set(None);
                                        let mut definition = definition;
                                        let mut validation = validation;
                                        let mut import_error = import_error;
                                        let mut import_open = import_open;
                                        let mut importing = importing;
                                        let mut notice = notice;
                                        spawn(async move {
                                            let result = async {
                                                let response = gloo_net::http::Request::get(&url)
                                                    .send()
                                                    .await
                                                    .map_err(|error| format!("Could not fetch URL: {error}. Check the URL and the host's CORS settings."))?;
                                                if !response.ok() {
                                                    return Err(format!("The URL returned HTTP {}", response.status()));
                                                }
                                                response.text().await.map_err(|error| format!("Could not read response: {error}"))
                                            }.await.and_then(|source| normalize_import(&source));
                                            match result {
                                                Ok((json, state)) => {
                                                    definition.set(json);
                                                    validation.set(state);
                                                    notice.set(None);
                                                    import_open.set(false);
                                                }
                                                Err(error) => import_error.set(Some(error)),
                                            }
                                            importing.set(false);
                                        });
                                    },
                                    if importing() { "Importing…" } else { "Import workflow" }
                                }
                                button {
                                    class: "button",
                                    r#type: "button",
                                    disabled: importing(),
                                    onclick: move |_| import_open.set(false),
                                    "Cancel"
                                }
                            }
                        }
                    }
                }
            }
            if input_open() {
                div { class: "workflow-import-overlay", role: "dialog", aria_modal: "true", aria_label: "Workflow input",
                    section { class: "workflow-import-modal workflow-input-modal",
                        header { class: "workflow-import-modal__header",
                            div {
                                p { class: "workflow-editor__eyebrow", "RUN CONFIGURATION" }
                                h2 { "Workflow input" }
                                p { "Provide the JSON value passed to the workflow." }
                            }
                            button {
                                class: "workflow-editor__icon-action",
                                r#type: "button",
                                aria_label: "Close input dialog",
                                onclick: move |_| input_open.set(false),
                                "×"
                            }
                        }
                        div { class: "workflow-import-modal__body",
                            label { class: "field workflow-input-field",
                                span { "Input JSON" }
                                textarea {
                                    class: "code-editor workflow-input-editor",
                                    value: "{input_draft}",
                                    spellcheck: "false",
                                    oninput: move |event| {
                                        input_draft.set(event.value());
                                        input_error.set(None);
                                    },
                                }
                            }
                            p { class: "workflow-import-help", "Use an empty JSON object when the workflow does not need input. Values are only used in this browser session." }
                            if let Some(error) = input_error() {
                                p { class: "workflow-import-error", role: "alert", "{error}" }
                            }
                            div { class: "workflow-import-actions",
                                button {
                                    class: "button button-primary",
                                    r#type: "button",
                                    onclick: move |_| match serde_json::from_str::<Value>(&input_draft()) {
                                        Ok(_) => {
                                            run_input.set(input_draft());
                                            input_error.set(None);
                                            input_open.set(false);
                                        }
                                        Err(error) => input_error.set(Some(format!("Input must be valid JSON: {error}"))),
                                    },
                                    "Save input"
                                }
                                button {
                                    class: "button",
                                    r#type: "button",
                                    onclick: move |_| input_open.set(false),
                                    "Cancel"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_example_loads_as_an_ows_definition() {
        for (id, _, source) in WORKFLOW_EXAMPLES {
            let (definition, validation) = normalize_import(source)
                .unwrap_or_else(|error| panic!("example `{id}` failed to load: {error}"));
            assert!(
                validation.valid,
                "example `{id}` failed OWS validation: {:?}",
                validation.errors
            );
            let json: Value = serde_json::from_str(&definition).expect("normalized example JSON");
            assert!(
                json["document"].is_object(),
                "example `{id}` has no document metadata"
            );
            assert!(json["do"].is_array(), "example `{id}` has no task list");
        }
    }

    #[test]
    fn every_example_expression_parses() {
        fn check(value: &Value, location: &str) {
            match value {
                Value::String(source)
                    if source.trim_start().starts_with("${")
                        && source.trim_end().ends_with('}') =>
                {
                    let expression = source
                        .trim()
                        .strip_prefix("${")
                        .unwrap()
                        .strip_suffix('}')
                        .unwrap()
                        .trim();
                    ows_expressions::parse(expression).unwrap_or_else(|error| {
                        panic!("expression at {location} did not parse: {error}")
                    });
                }
                Value::Array(items) => {
                    for (index, item) in items.iter().enumerate() {
                        check(item, &format!("{location}/{index}"));
                    }
                }
                Value::Object(fields) => {
                    for (key, item) in fields {
                        check(item, &format!("{location}/{key}"));
                    }
                }
                _ => {}
            }
        }
        for (id, _, source) in WORKFLOW_EXAMPLES {
            let value: Value = serde_json::from_str(&normalize_import(source).unwrap().0).unwrap();
            check(&value, id);
        }
    }

    #[test]
    fn http_workflow_interpolations_use_input_and_response_values() {
        let input = json!({ "novelSlug": "sample-novel" });
        let endpoint = evaluate_expression(
            r#"${ "https://brightnovels.com/series/" + .novelSlug + "/chapters" }"#,
            &input,
            &Map::new(),
        )
        .unwrap();
        assert_eq!(endpoint, "https://brightnovels.com/series/sample-novel/chapters");

        let input = json!({ "novel_url": "https://brightnovels.com/series/sample-novel" });
        let response = Value::String("series html".to_string());
        let mut variables = Map::new();
        variables.insert("output".to_string(), response.clone());
        let transformed = evaluate_expression(
            "$input + { seriesHtml: $output }",
            &input,
            &variables,
        )
        .unwrap();
        assert_eq!(transformed["seriesHtml"], response);
        assert_eq!(transformed["novel_url"], input["novel_url"]);
    }

    #[test]
    fn local_examples_execute_in_the_browser_runner_model() {
        assert!(
            validate_ows(STARTER_WORKFLOW).valid,
            "the starter Hello workflow must pass validation so Run is enabled"
        );
        for (id, _, source) in WORKFLOW_EXAMPLES {
            if matches!(*id, "call" | "download_novel" | "raise") {
                continue;
            }
            let (definition, _) = normalize_import(source).expect("example should load");
            let result = futures_executor::block_on(run_client_workflow(&definition, json!({})));
            assert!(
                result.is_ok(),
                "example `{id}` failed to execute: {result:?}"
            );
        }
        let (_, _, source) = WORKFLOW_EXAMPLES
            .iter()
            .find(|(id, _, _)| *id == "raise")
            .unwrap();
        let (definition, _) = normalize_import(source).expect("raise example should load");
        assert!(futures_executor::block_on(run_client_workflow(&definition, json!({}))).is_err());
    }
}
