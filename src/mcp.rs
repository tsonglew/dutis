use crate::application::{find_apps_for_extension, normalize_extension, ApplicationCatalog};
use crate::config::DutisConfig;
use crate::planner::{build_plan, AssociationPlan};
use crate::snapshot::{
    apply_plan_with_snapshot, build_rollback_plan, SnapshotReason, SnapshotStore,
};
use crate::system;
use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::io::{BufRead, Write};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

pub const MCP_PROTOCOL_VERSION: &str = "2025-06-18";
pub const MCP_AUDIT_SCHEMA_VERSION: u32 = 1;
const TOOL_API_VERSION: &str = "1";
const APPROVAL_TOKEN_ENV: &str = "DUTIS_MCP_APPROVAL_TOKEN";
const MAX_CONFIG_BYTES: usize = 1_048_576;

pub struct McpOptions {
    allow_writes: bool,
    approval_token: Option<String>,
}

impl McpOptions {
    pub fn read_only() -> Self {
        Self {
            allow_writes: false,
            approval_token: None,
        }
    }

    pub fn from_environment(allow_writes: bool) -> Result<Self> {
        if !allow_writes {
            return Ok(Self::read_only());
        }
        let token = std::env::var(APPROVAL_TOKEN_ENV).with_context(|| {
            format!("{APPROVAL_TOKEN_ENV} must be set when --allow-writes is enabled")
        })?;
        if token.len() < 16 {
            return Err(anyhow!(
                "{APPROVAL_TOKEN_ENV} must contain at least 16 characters"
            ));
        }
        Ok(Self {
            allow_writes: true,
            approval_token: Some(token),
        })
    }

    #[cfg(test)]
    fn with_writes(token: &str) -> Self {
        Self {
            allow_writes: true,
            approval_token: Some(token.to_owned()),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct McpAuditEvent {
    pub schema_version: u32,
    pub timestamp: String,
    pub request_id: Value,
    pub tool: String,
    pub access: &'static str,
    pub outcome: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_kind: Option<String>,
}

#[derive(Serialize)]
struct MutationResult {
    safety_snapshot_id: Option<String>,
    #[serde(flatten)]
    report: crate::planner::ApplyReport,
}

trait McpBackend {
    fn list(&mut self) -> Result<Value>;
    fn query(&mut self, extension: &str) -> Result<Value>;
    fn get(&mut self, extension: &str) -> Result<Value>;
    fn plan(&mut self, config: &DutisConfig) -> Result<AssociationPlan>;
    fn apply(&mut self, plan: &AssociationPlan, reason: SnapshotReason) -> Result<Value>;
    fn history(&mut self) -> Result<Value>;
    fn rollback_plan(&mut self, snapshot_id: &str) -> Result<AssociationPlan>;
}

struct SystemBackend;

impl McpBackend for SystemBackend {
    fn list(&mut self) -> Result<Value> {
        let catalog = ApplicationCatalog::scan()?;
        Ok(json!({
            "applications": catalog.applications,
            "metadata_failures": catalog.metadata_failures,
        }))
    }

    fn query(&mut self, extension: &str) -> Result<Value> {
        let catalog = ApplicationCatalog::scan()?;
        let applications = find_apps_for_extension(&catalog.applications, extension);
        Ok(json!({
            "extension": extension,
            "applications": applications,
            "metadata_failures": catalog.metadata_failures,
        }))
    }

    fn get(&mut self, extension: &str) -> Result<Value> {
        let default = system::get_default_app(extension)?;
        Ok(json!({"extension": extension, "default": default}))
    }

    fn plan(&mut self, config: &DutisConfig) -> Result<AssociationPlan> {
        system::duti_version()?;
        let catalog = ApplicationCatalog::scan()?;
        build_plan(config, &catalog.applications, system::query_default_app)
    }

    fn apply(&mut self, plan: &AssociationPlan, reason: SnapshotReason) -> Result<Value> {
        let store = SnapshotStore::from_environment()?;
        let protected = apply_plan_with_snapshot(&store, plan, reason, system::set_default_app)?;
        serde_json::to_value(MutationResult {
            safety_snapshot_id: protected.safety_snapshot.map(|snapshot| snapshot.id),
            report: protected.report,
        })
        .context("failed to serialize mutation result")
    }

    fn history(&mut self) -> Result<Value> {
        let history = SnapshotStore::from_environment()?.history()?;
        serde_json::to_value(history).context("failed to serialize snapshot history")
    }

    fn rollback_plan(&mut self, snapshot_id: &str) -> Result<AssociationPlan> {
        let store = SnapshotStore::from_environment()?;
        let snapshot = store.load(snapshot_id)?;
        system::duti_version()?;
        let catalog = ApplicationCatalog::scan()?;
        build_rollback_plan(&snapshot, &catalog.applications, system::query_default_app)
    }
}

struct ToolError {
    kind: &'static str,
    message: String,
    details: Option<Value>,
}

impl ToolError {
    fn new(kind: &'static str, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            details: None,
        }
    }

    fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }
}

struct HandleOutcome {
    response: Option<Value>,
    audit: Option<McpAuditEvent>,
}

struct McpServer<B> {
    backend: B,
    options: McpOptions,
}

impl<B: McpBackend> McpServer<B> {
    fn new(backend: B, options: McpOptions) -> Self {
        Self { backend, options }
    }

    fn handle(&mut self, message: Value) -> HandleOutcome {
        let Some(object) = message.as_object() else {
            return response_only(json_rpc_error(Value::Null, -32600, "invalid request"));
        };
        let id = object.get("id").cloned();
        let Some(method) = object.get("method").and_then(Value::as_str) else {
            return response_only(json_rpc_error(
                id.unwrap_or(Value::Null),
                -32600,
                "invalid request",
            ));
        };

        if id.is_none() {
            return HandleOutcome {
                response: None,
                audit: None,
            };
        }
        let id = id.unwrap_or(Value::Null);
        let params = object.get("params").cloned().unwrap_or_else(|| json!({}));

        match method {
            "initialize" => response_only(json_rpc_result(
                id,
                json!({
                    "protocolVersion": negotiated_protocol_version(&params),
                    "capabilities": {"tools": {"listChanged": false}},
                    "serverInfo": {
                        "name": "dutis",
                        "version": env!("CARGO_PKG_VERSION"),
                    },
                    "instructions": "Inspect and plan first. Mutation tools require server-side write enablement, a fresh plan digest, and an approval token."
                }),
            )),
            "ping" => response_only(json_rpc_result(id, json!({}))),
            "tools/list" => response_only(json_rpc_result(
                id,
                json!({"tools": tool_definitions(self.options.allow_writes)}),
            )),
            "tools/call" => self.call_tool(id, params),
            _ => response_only(json_rpc_error(id, -32601, "method not found")),
        }
    }

    fn call_tool(&mut self, id: Value, params: Value) -> HandleOutcome {
        let Some(name) = params.get("name").and_then(Value::as_str) else {
            return response_only(json_rpc_error(id, -32602, "tool name is required"));
        };
        let arguments = params
            .get("arguments")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let access = if is_write_tool(name) { "write" } else { "read" };
        let result = self.dispatch_tool(name, &arguments);
        let (tool_result, outcome, error_kind) = match result {
            Ok(data) => (tool_success(data), "success", None),
            Err(error) => {
                let kind = error.kind.to_owned();
                (tool_error(error), "error", Some(kind))
            }
        };
        HandleOutcome {
            response: Some(json_rpc_result(id.clone(), tool_result)),
            audit: Some(McpAuditEvent {
                schema_version: MCP_AUDIT_SCHEMA_VERSION,
                timestamp: timestamp(),
                request_id: id,
                tool: name.to_owned(),
                access,
                outcome,
                error_kind,
            }),
        }
    }

    fn dispatch_tool(
        &mut self,
        name: &str,
        arguments: &Map<String, Value>,
    ) -> std::result::Result<Value, ToolError> {
        match name {
            "dutis_list" => self.backend.list().map_err(operation_error),
            "dutis_query" => {
                let extension = argument_string(arguments, "extension")?;
                let extension = normalize_extension(extension)
                    .map_err(|error| ToolError::new("invalid_arguments", error.to_string()))?;
                self.backend.query(&extension).map_err(operation_error)
            }
            "dutis_get" => {
                let extension = argument_string(arguments, "extension")?;
                let extension = normalize_extension(extension)
                    .map_err(|error| ToolError::new("invalid_arguments", error.to_string()))?;
                self.backend.get(&extension).map_err(operation_error)
            }
            "dutis_diff" => {
                let config = parse_config(arguments)?;
                let plan = self.backend.plan(&config).map_err(operation_error)?;
                serde_json::to_value(plan).map_err(|error| operation_error(error.into()))
            }
            "dutis_history" => self.backend.history().map_err(operation_error),
            "dutis_rollback_plan" => {
                let snapshot_id = argument_string(arguments, "snapshot_id")?;
                let plan = self
                    .backend
                    .rollback_plan(snapshot_id)
                    .map_err(operation_error)?;
                serde_json::to_value(plan).map_err(|error| operation_error(error.into()))
            }
            "dutis_apply" => {
                self.authorize_write(arguments)?;
                let config = parse_config(arguments)?;
                let expected_digest = argument_string(arguments, "plan_digest")?;
                let plan = self.backend.plan(&config).map_err(operation_error)?;
                validate_mutation_plan(&plan, expected_digest)?;
                let result = self
                    .backend
                    .apply(&plan, SnapshotReason::BeforeApply)
                    .map_err(operation_error)?;
                validate_apply_result(result)
            }
            "dutis_rollback" => {
                self.authorize_write(arguments)?;
                let snapshot_id = argument_string(arguments, "snapshot_id")?;
                let expected_digest = argument_string(arguments, "plan_digest")?;
                let plan = self
                    .backend
                    .rollback_plan(snapshot_id)
                    .map_err(operation_error)?;
                validate_mutation_plan(&plan, expected_digest)?;
                let result = self
                    .backend
                    .apply(&plan, SnapshotReason::BeforeRollback)
                    .map_err(operation_error)?;
                validate_apply_result(result)
            }
            _ => Err(ToolError::new(
                "tool_not_found",
                format!("unknown tool '{name}'"),
            )),
        }
    }

    fn authorize_write(
        &self,
        arguments: &Map<String, Value>,
    ) -> std::result::Result<(), ToolError> {
        if !self.options.allow_writes {
            return Err(ToolError::new(
                "write_disabled",
                "mutation tools are disabled; restart with --allow-writes",
            ));
        }
        let provided = argument_string(arguments, "approval_token")?;
        let expected = self
            .options
            .approval_token
            .as_deref()
            .ok_or_else(|| ToolError::new("write_disabled", "approval token is unavailable"))?;
        if !tokens_match(expected, provided) {
            return Err(ToolError::new(
                "approval_denied",
                "approval token does not match",
            ));
        }
        Ok(())
    }
}

fn validate_mutation_plan(
    plan: &AssociationPlan,
    expected_digest: &str,
) -> std::result::Result<(), ToolError> {
    if plan.digest != expected_digest {
        return Err(ToolError::new(
            "stale_plan",
            "current plan digest differs from the reviewed digest",
        )
        .with_details(json!({
            "expected_plan_digest": expected_digest,
            "current_plan": plan,
        })));
    }
    if plan.has_unresolved() {
        return Err(ToolError::new(
            "unresolved_plan",
            format!(
                "plan contains {} unresolved association(s); no changes were made",
                plan.summary.unresolved
            ),
        )
        .with_details(json!({"plan": plan})));
    }
    Ok(())
}

fn validate_apply_result(result: Value) -> std::result::Result<Value, ToolError> {
    let failed = result.get("failed").and_then(Value::as_u64).unwrap_or(0);
    if failed > 0 {
        return Err(ToolError::new(
            "partial_failure",
            format!("{failed} association(s) failed; safety snapshot retained"),
        )
        .with_details(result));
    }
    Ok(result)
}

fn argument_string<'a>(
    arguments: &'a Map<String, Value>,
    name: &str,
) -> std::result::Result<&'a str, ToolError> {
    arguments
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ToolError::new(
                "invalid_arguments",
                format!("'{name}' must be a non-empty string"),
            )
        })
}

fn parse_config(arguments: &Map<String, Value>) -> std::result::Result<DutisConfig, ToolError> {
    let contents = argument_string(arguments, "config_toml")?;
    if contents.len() > MAX_CONFIG_BYTES {
        return Err(ToolError::new(
            "invalid_arguments",
            "'config_toml' exceeds the 1 MiB limit",
        ));
    }
    DutisConfig::parse(contents)
        .map_err(|error| ToolError::new("invalid_arguments", format!("{error:#}")))
}

fn operation_error(error: anyhow::Error) -> ToolError {
    ToolError::new("operation_failed", format!("{error:#}"))
}

fn tokens_match(expected: &str, provided: &str) -> bool {
    let expected = Sha256::digest(expected.as_bytes());
    let provided = Sha256::digest(provided.as_bytes());
    expected
        .iter()
        .zip(provided.iter())
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

fn timestamp() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned())
}

fn negotiated_protocol_version(params: &Value) -> &str {
    match params.get("protocolVersion").and_then(Value::as_str) {
        Some("2024-11-05") => "2024-11-05",
        Some("2025-03-26") => "2025-03-26",
        Some("2025-06-18") => "2025-06-18",
        _ => MCP_PROTOCOL_VERSION,
    }
}

fn response_only(response: Value) -> HandleOutcome {
    HandleOutcome {
        response: Some(response),
        audit: None,
    }
}

fn json_rpc_result(id: Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn json_rpc_error(id: Value, code: i32, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {"code": code, "message": message},
    })
}

fn tool_success(data: Value) -> Value {
    let envelope = json!({"api_version": TOOL_API_VERSION, "data": data});
    let text = serde_json::to_string_pretty(&envelope)
        .unwrap_or_else(|_| "{\"api_version\":\"1\"}".to_owned());
    json!({
        "content": [{"type": "text", "text": text}],
        "structuredContent": envelope,
        "isError": false,
    })
}

fn tool_error(error: ToolError) -> Value {
    let mut body = json!({
        "kind": error.kind,
        "message": error.message,
    });
    if let Some(details) = error.details {
        body["details"] = details;
    }
    let envelope = json!({"api_version": TOOL_API_VERSION, "error": body});
    let text = serde_json::to_string_pretty(&envelope)
        .unwrap_or_else(|_| "{\"api_version\":\"1\"}".to_owned());
    json!({
        "content": [{"type": "text", "text": text}],
        "structuredContent": envelope,
        "isError": true,
    })
}

fn is_write_tool(name: &str) -> bool {
    matches!(name, "dutis_apply" | "dutis_rollback")
}

fn tool_definitions(allow_writes: bool) -> Vec<Value> {
    let empty_schema = json!({"type": "object", "properties": {}, "additionalProperties": false});
    let extension_schema = json!({
        "type": "object",
        "properties": {"extension": {"type": "string", "minLength": 1}},
        "required": ["extension"],
        "additionalProperties": false,
    });
    let config_schema = json!({
        "type": "object",
        "properties": {"config_toml": {"type": "string", "minLength": 1}},
        "required": ["config_toml"],
        "additionalProperties": false,
    });
    let snapshot_schema = json!({
        "type": "object",
        "properties": {"snapshot_id": {"type": "string", "minLength": 1}},
        "required": ["snapshot_id"],
        "additionalProperties": false,
    });
    let read_annotations = json!({
        "readOnlyHint": true,
        "destructiveHint": false,
        "idempotentHint": true,
        "openWorldHint": false,
    });
    let mut tools = vec![
        tool_definition(
            "dutis_list",
            "List installed macOS applications and their declared file extensions.",
            empty_schema.clone(),
            read_annotations.clone(),
        ),
        tool_definition(
            "dutis_query",
            "Find installed applications that declare support for an extension.",
            extension_schema.clone(),
            read_annotations.clone(),
        ),
        tool_definition(
            "dutis_get",
            "Read the current default application for an extension.",
            extension_schema,
            read_annotations.clone(),
        ),
        tool_definition(
            "dutis_diff",
            "Build a deterministic association plan and digest from TOML without changing the system.",
            config_schema.clone(),
            read_annotations.clone(),
        ),
        tool_definition(
            "dutis_history",
            "List locally stored safety snapshots in newest-first order.",
            empty_schema,
            read_annotations.clone(),
        ),
        tool_definition(
            "dutis_rollback_plan",
            "Build a deterministic rollback plan and digest without changing the system.",
            snapshot_schema.clone(),
            read_annotations,
        ),
    ];
    if allow_writes {
        let write_annotations = json!({
            "readOnlyHint": false,
            "destructiveHint": true,
            "idempotentHint": false,
            "openWorldHint": false,
        });
        tools.push(tool_definition(
            "dutis_apply",
            "Apply and verify a freshly reviewed TOML plan. Requires a matching digest and approval token.",
            json!({
                "type": "object",
                "properties": {
                    "config_toml": {"type": "string", "minLength": 1},
                    "plan_digest": {"type": "string", "minLength": 1},
                    "approval_token": {"type": "string", "minLength": 1}
                },
                "required": ["config_toml", "plan_digest", "approval_token"],
                "additionalProperties": false,
            }),
            write_annotations.clone(),
        ));
        tools.push(tool_definition(
            "dutis_rollback",
            "Apply and verify a freshly reviewed rollback plan. Requires a matching digest and approval token.",
            json!({
                "type": "object",
                "properties": {
                    "snapshot_id": {"type": "string", "minLength": 1},
                    "plan_digest": {"type": "string", "minLength": 1},
                    "approval_token": {"type": "string", "minLength": 1}
                },
                "required": ["snapshot_id", "plan_digest", "approval_token"],
                "additionalProperties": false,
            }),
            write_annotations,
        ));
    }
    tools
}

fn tool_definition(
    name: &str,
    description: &str,
    input_schema: Value,
    annotations: Value,
) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": input_schema,
        "outputSchema": {
            "type": "object",
            "properties": {"api_version": {"const": TOOL_API_VERSION}},
            "required": ["api_version"],
            "additionalProperties": true
        },
        "annotations": annotations,
    })
}

pub fn serve_stdio(options: McpOptions) -> Result<()> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let stderr = std::io::stderr();
    serve(
        stdin.lock(),
        stdout.lock(),
        stderr.lock(),
        McpServer::new(SystemBackend, options),
    )
}

fn serve<R, W, E, B>(
    reader: R,
    mut writer: W,
    mut audit_writer: E,
    mut server: McpServer<B>,
) -> Result<()>
where
    R: BufRead,
    W: Write,
    E: Write,
    B: McpBackend,
{
    for line in reader.lines() {
        let line = line.context("failed to read MCP input")?;
        if line.trim().is_empty() {
            continue;
        }
        let outcome = match serde_json::from_str::<Value>(&line) {
            Ok(message) => server.handle(message),
            Err(_) => response_only(json_rpc_error(Value::Null, -32700, "parse error")),
        };
        if let Some(audit) = outcome.audit {
            serde_json::to_writer(&mut audit_writer, &audit)?;
            audit_writer.write_all(b"\n")?;
            audit_writer.flush()?;
        }
        if let Some(response) = outcome.response {
            serde_json::to_writer(&mut writer, &response)?;
            writer.write_all(b"\n")?;
            writer.flush()?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planner::{ApplyReport, PlanSummary};

    struct FakeBackend {
        plan: AssociationPlan,
        apply_calls: usize,
    }

    impl FakeBackend {
        fn new() -> Self {
            Self {
                plan: AssociationPlan {
                    schema_version: 1,
                    config_version: 1,
                    digest: "reviewed-digest".to_owned(),
                    summary: PlanSummary {
                        total: 0,
                        changes: 0,
                        unchanged: 0,
                        unresolved: 0,
                    },
                    entries: Vec::new(),
                },
                apply_calls: 0,
            }
        }
    }

    impl McpBackend for FakeBackend {
        fn list(&mut self) -> Result<Value> {
            Ok(json!({"applications": []}))
        }

        fn query(&mut self, extension: &str) -> Result<Value> {
            Ok(json!({"extension": extension, "applications": []}))
        }

        fn get(&mut self, extension: &str) -> Result<Value> {
            Ok(json!({"extension": extension, "default": null}))
        }

        fn plan(&mut self, _config: &DutisConfig) -> Result<AssociationPlan> {
            Ok(self.plan.clone())
        }

        fn apply(&mut self, plan: &AssociationPlan, _reason: SnapshotReason) -> Result<Value> {
            self.apply_calls += 1;
            serde_json::to_value(MutationResult {
                safety_snapshot_id: None,
                report: ApplyReport {
                    plan_digest: plan.digest.clone(),
                    applied: 0,
                    skipped: 0,
                    failed: 0,
                    results: Vec::new(),
                },
            })
            .map_err(Into::into)
        }

        fn history(&mut self) -> Result<Value> {
            Ok(json!([]))
        }

        fn rollback_plan(&mut self, _snapshot_id: &str) -> Result<AssociationPlan> {
            Ok(self.plan.clone())
        }
    }

    fn request(id: u64, method: &str, params: Value) -> Value {
        json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
    }

    fn config_toml() -> &'static str {
        "version = 1\n[associations]\nmd = 'com.example.Editor'\n"
    }

    #[test]
    fn initializes_with_stable_protocol_metadata() {
        let mut server = McpServer::new(FakeBackend::new(), McpOptions::read_only());
        let outcome = server.handle(request(1, "initialize", json!({})));
        let response = outcome.response.unwrap();
        assert_eq!(response["result"]["protocolVersion"], MCP_PROTOCOL_VERSION);
        assert_eq!(response["result"]["serverInfo"]["name"], "dutis");
    }

    #[test]
    fn read_only_mode_does_not_advertise_write_tools() {
        let mut server = McpServer::new(FakeBackend::new(), McpOptions::read_only());
        let response = server
            .handle(request(1, "tools/list", json!({})))
            .response
            .unwrap();
        let names = response["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert!(names.contains(&"dutis_diff"));
        assert!(!names.contains(&"dutis_apply"));
        assert!(!names.contains(&"dutis_rollback"));
    }

    #[test]
    fn write_mode_advertises_mutations() {
        let mut server = McpServer::new(
            FakeBackend::new(),
            McpOptions::with_writes("a-secure-test-token"),
        );
        let response = server
            .handle(request(1, "tools/list", json!({})))
            .response
            .unwrap();
        let names = response["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert!(names.contains(&"dutis_apply"));
        assert!(names.contains(&"dutis_rollback"));
    }

    #[test]
    fn advertised_schemas_are_closed_and_versioned() {
        for tool in tool_definitions(true) {
            assert_eq!(tool["inputSchema"]["additionalProperties"], false);
            assert_eq!(
                tool["outputSchema"]["properties"]["api_version"]["const"],
                "1"
            );
            assert_eq!(tool["outputSchema"]["required"], json!(["api_version"]));
        }
    }

    #[test]
    fn disabled_write_returns_a_stable_error_and_audit_event() {
        let mut server = McpServer::new(FakeBackend::new(), McpOptions::read_only());
        let outcome = server.handle(request(
            9,
            "tools/call",
            json!({
                "name": "dutis_apply",
                "arguments": {
                    "config_toml": config_toml(),
                    "plan_digest": "reviewed-digest",
                    "approval_token": "a-secure-test-token"
                }
            }),
        ));
        let response = outcome.response.unwrap();
        assert_eq!(response["result"]["isError"], true);
        assert_eq!(
            response["result"]["structuredContent"]["error"]["kind"],
            "write_disabled"
        );
        assert_eq!(server.backend.apply_calls, 0);
        let audit = outcome.audit.unwrap();
        assert_eq!(audit.outcome, "error");
        assert_eq!(audit.error_kind.as_deref(), Some("write_disabled"));
    }

    #[test]
    fn approval_and_digest_are_checked_before_mutation() {
        let mut server = McpServer::new(
            FakeBackend::new(),
            McpOptions::with_writes("a-secure-test-token"),
        );
        let denied = server.handle(request(
            1,
            "tools/call",
            json!({
                "name": "dutis_apply",
                "arguments": {
                    "config_toml": config_toml(),
                    "plan_digest": "reviewed-digest",
                    "approval_token": "wrong-token"
                }
            }),
        ));
        assert_eq!(
            denied.response.unwrap()["result"]["structuredContent"]["error"]["kind"],
            "approval_denied"
        );
        assert_eq!(server.backend.apply_calls, 0);

        let stale = server.handle(request(
            2,
            "tools/call",
            json!({
                "name": "dutis_apply",
                "arguments": {
                    "config_toml": config_toml(),
                    "plan_digest": "old-digest",
                    "approval_token": "a-secure-test-token"
                }
            }),
        ));
        assert_eq!(
            stale.response.unwrap()["result"]["structuredContent"]["error"]["kind"],
            "stale_plan"
        );
        assert_eq!(server.backend.apply_calls, 0);
    }

    #[test]
    fn approved_fresh_plan_mutates_and_emits_audit_event() {
        let mut server = McpServer::new(
            FakeBackend::new(),
            McpOptions::with_writes("a-secure-test-token"),
        );
        let outcome = server.handle(request(
            7,
            "tools/call",
            json!({
                "name": "dutis_apply",
                "arguments": {
                    "config_toml": config_toml(),
                    "plan_digest": "reviewed-digest",
                    "approval_token": "a-secure-test-token"
                }
            }),
        ));
        assert_eq!(server.backend.apply_calls, 1);
        assert_eq!(outcome.response.unwrap()["result"]["isError"], false);
        let audit = outcome.audit.unwrap();
        assert_eq!(audit.schema_version, 1);
        assert_eq!(audit.request_id, json!(7));
        assert_eq!(audit.tool, "dutis_apply");
        assert_eq!(audit.access, "write");
        assert_eq!(audit.outcome, "success");
    }

    #[test]
    fn stdio_transport_returns_parse_errors_and_ignores_notifications() {
        let input = b"not-json\n{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n";
        let mut output = Vec::new();
        let mut audit = Vec::new();
        serve(
            &input[..],
            &mut output,
            &mut audit,
            McpServer::new(FakeBackend::new(), McpOptions::read_only()),
        )
        .unwrap();
        let lines = String::from_utf8(output).unwrap();
        assert_eq!(lines.lines().count(), 1);
        let response: Value = serde_json::from_str(lines.trim()).unwrap();
        assert_eq!(response["error"]["code"], -32700);
        assert!(audit.is_empty());
    }
}
