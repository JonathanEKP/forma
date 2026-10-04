//! A paired Pontis Runtime with FakeProvider, for the sidecar's local run.
//!
//! It does what a user's machine does — generate a Device key, claim a pairing
//! code, hold a credential, open the device socket, hand intents to the Runtime
//! — and nothing else. Every piece is a public Pontis crate wired the way
//! Pontis's own remote tests wire them; none of it is reimplemented here.
//!
//! What it adds is the part only Forma can write: the FakeProvider scripts,
//! keyed by exact input, naming real Forma ids handed in by the test that
//! seeded them. Assertions live in that test (`src/local-run.test.ts`), which
//! plays the person: it starts sessions, sends input, cancels and approves
//! through the sidecar's routes, and reads what happened from the durable event
//! stream and from Forma itself.
//!
//! Prints `READY <device id>` once the socket is up, and exits on SIGTERM or
//! when stdin closes, so it cannot outlive the test that started it.
//!
//! Two providers, chosen by `FORMA_DRIVER_PROVIDER`: `fake` (the default, and
//! the only one CI-adjacent runs use) and `codex`, the opt-in real run of plan
//! §11 step 11. The Codex mode is `capability_only` — it is given the Runtime's
//! MCP boundary, which is what selects that execution policy — and never the
//! provider-native compatibility mode, which claims no posture and so is not
//! evidence of anything. It runs the executable and `CODEX_HOME` the caller
//! names, so a run never touches the owner's own install or configuration.

use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use pontis_identity::key::DeviceKeyPair;
use pontis_protocol::device::DeviceMetadataBody;
use pontis_protocol::errors::TypedError;
use pontis_protocol::provider::ProviderDriver;
use pontis_provider_codex::CodexProvider;
use pontis_provider_fake::{ExpectedOutcome, FakeProvider, FakeScript};
use pontis_runtime::{
    ApprovalManager, ChannelEventSink, DelegationSource, HttpsGatewayClient, IntentObserver,
    ProviderRouter, ProviderWorkspaces, RemoteAgent, RemoteApprovals, RemoteDelegationIssuer,
    RiskPolicy, SessionDelegations, SessionManager, SessionPostureReport,
};
use pontis_transport::channel::DeviceChannel;
use pontis_transport::credentials::CredentialHolder;
use pontis_transport::device_client::DeviceClient;
use pontis_transport::endpoint::Endpoint;
use pontis_transport::remote::{RemoteChannel, RemoteChannelOptions};
use serde_json::json;
use tokio::io::AsyncReadExt;

/// The exact inputs the test sends. FakeProvider matches them byte for byte.
const READ: &str = "Which projects do I have, and what is in my design?";
const WRITE: &str = "Raise the headline to 96pt.";
const CANCEL: &str = "Validate the long design.";
const DENY: &str = "Remove the tagline. I will say no.";
const APPROVE: &str = "Remove the headline.";
const GUIDE: &str = "Read the manual first.";
const BRAND: &str = "Read my brand, and ask me for a hero photo.";

/// Prints what the Runtime refused, one line each, so the test can read it.
///
/// A refusal is the only place some outcomes surface: a turn whose provider
/// stopped listening mid-capability ends there rather than in a durable event
/// (see `local-run.test.ts`, the cancel scenario). Code and message only — a
/// typed error carries neither a credential nor a delegation.
struct Refusals;

impl IntentObserver for Refusals {
    fn refused(&self, error: TypedError) {
        eprintln!("REFUSED {} {}", error.code, error.message);
    }
}

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is required"))
}

/// One script per scenario. The ids and the version are the seeded design's,
/// so each batch is one Forma would accept.
///
/// The refused removal and the approved one name different layers: Pontis
/// remembers a refusal of the identical action for the rest of the session and
/// will not ask again, which is the behaviour a person would want.
fn provider(
    project: &str,
    design: &str,
    long_design: &str,
    layer: &str,
    tagline: &str,
    version: u64,
) -> FakeProvider {
    let removal = |layer: &str| {
        json!({
            "design_id": design,
            "base_version": version + 1,
            "operations": [{ "op": "remove_layer", "layer_id": layer }],
        })
    };
    FakeProvider::builder()
        .script(
            READ,
            FakeScript::new()
                .call("project.list", json!({}))
                .expect_tool_result_status(ExpectedOutcome::Completed)
                .call("design.get", json!({ "design_id": design }))
                .expect_tool_result_status(ExpectedOutcome::Completed)
                .emit("You have one project, and the design has a headline.")
                .complete(),
        )
        .script(
            GUIDE,
            FakeScript::new()
                .call("forma.guide", json!({}))
                .expect_tool_result_status(ExpectedOutcome::Completed)
                .emit("I have read the manual.")
                .complete(),
        )
        .script(
            BRAND,
            FakeScript::new()
                .call("brand.get", json!({ "project_id": project }))
                .expect_tool_result_status(ExpectedOutcome::Completed)
                .call(
                    "assetRequest.create",
                    json!({
                        "project_id": project,
                        "design_id": design,
                        "purpose": "hero photo",
                        "description": "A wet street at dusk, with room for a headline at the top.",
                    }),
                )
                .expect_tool_result_status(ExpectedOutcome::Completed)
                .emit("I read the brand and asked you for a hero photo.")
                .complete(),
        )
        .script(
            WRITE,
            FakeScript::new()
                .call(
                    "design.applyOperations",
                    json!({
                        "design_id": design,
                        "base_version": version,
                        "operations": [
                            { "op": "patch_text_style", "layer_id": layer, "font_size": 96.0 }
                        ],
                    }),
                )
                .expect_tool_result_status(ExpectedOutcome::Completed)
                .emit("The headline is 96pt now.")
                .complete(),
        )
        .script(
            CANCEL,
            FakeScript::new()
                .call("design.validate", json!({ "design_id": long_design }))
                .expect_tool_result()
                .emit("Validation finished.")
                .complete(),
        )
        .script(
            DENY,
            FakeScript::new()
                .call("design.applyOperations", removal(tagline))
                .expect_tool_result_status(ExpectedOutcome::Failed)
                .emit("You declined, so the tagline stays.")
                .complete(),
        )
        .script(
            APPROVE,
            FakeScript::new()
                .call("design.applyOperations", removal(layer))
                .expect_tool_result_status(ExpectedOutcome::Completed)
                .emit("The headline is gone.")
                .complete(),
        )
        .build()
}

/// Prints, once per session, the posture the Runtime verified before it let the session start:
/// `POSTURE <session id> <summary json>` on stderr. The summary is the bounded one the Runtime's
/// own status surface carries — provider version, platform, policy and each check's state — with
/// no guidance, path or provider output, so the step 11 evidence can say *what* was verified
/// rather than only that a session started.
async fn report_postures(sessions: Arc<SessionManager>) {
    let mut reported = std::collections::HashSet::new();
    loop {
        for id in sessions.session_ids() {
            if reported.contains(id.as_str()) {
                continue;
            }
            if let Ok(SessionPostureReport::Verified(summary)) = sessions.session_posture(&id) {
                if let Ok(line) = serde_json::to_string(&summary) {
                    eprintln!("POSTURE {} {line}", id.as_str());
                    reported.insert(id.as_str().to_owned());
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// The real provider: the Codex the caller pinned, in the `CODEX_HOME` the caller made, in
/// `capability_only` — `with_mcp_runtime` is what selects that execution policy, and the
/// provider-native compatibility mode is deliberately unreachable from here.
///
/// The environment is the parent's three variables Codex needs to start plus `CODEX_HOME`; the
/// provider applies its own allowlist on top and drops anything else. Nothing is read from the
/// auth store here: Codex finds its own credential through the home's `auth.json`.
fn codex_provider(runtime_dir: &std::path::Path) -> Arc<dyn ProviderDriver> {
    let mut environment: HashMap<String, String> = ["PATH", "HOME", "LANG", "TERM"]
        .into_iter()
        .filter_map(|name| {
            std::env::var(name)
                .ok()
                .map(|value| (name.to_owned(), value))
        })
        .collect();
    environment.insert("CODEX_HOME".into(), env("FORMA_DRIVER_CODEX_HOME"));
    Arc::new(
        CodexProvider::with_executable(PathBuf::from(env("FORMA_DRIVER_CODEX_BIN")))
            .with_environment(environment)
            .with_reasoning_effort("low")
            .with_mcp_runtime(PathBuf::from(env("FORMA_DRIVER_MCP_BRIDGE")), runtime_dir),
    )
}

#[tokio::main]
async fn main() {
    let origin = env("FORMA_PONTIS_ORIGIN");
    let application_id = env("FORMA_PONTIS_APPLICATION_ID");
    let code = env("FORMA_DRIVER_PAIRING_CODE");
    // Only FakeProvider's scripts name a version; a real model reads it from the design.
    let version: u64 = std::env::var("FORMA_DRIVER_BASE_VERSION").map_or(0, |value| {
        value
            .parse()
            .expect("FORMA_DRIVER_BASE_VERSION is a version number")
    });
    let codex = std::env::var("FORMA_DRIVER_PROVIDER").is_ok_and(|name| name == "codex");
    // The directory for the Runtime's per-session MCP sockets, which also holds the neutral
    // workspaces. Kept alive for the whole run: dropping it removes the sockets under a session.
    let workspace_root = tempfile::Builder::new()
        .prefix("p-mcp-")
        .tempdir()
        .expect("a runtime directory");
    std::fs::set_permissions(
        workspace_root.path(),
        std::fs::Permissions::from_mode(0o700),
    )
    .expect("the runtime directory is owner-only");
    let provider: Arc<dyn ProviderDriver> = if codex {
        codex_provider(workspace_root.path())
    } else {
        Arc::new(provider(
            &env("FORMA_DRIVER_PROJECT"),
            &env("FORMA_DRIVER_DESIGN"),
            &env("FORMA_DRIVER_LONG_DESIGN"),
            &env("FORMA_DRIVER_LAYER"),
            &env("FORMA_DRIVER_TAGLINE"),
            version,
        ))
    };

    let metadata = DeviceMetadataBody {
        name: "Forma local run".into(),
        platform: "linux-x86_64".into(),
        runtime_version: "test-driver".into(),
    };
    let endpoint = Endpoint::parse(&origin).expect("FORMA_PONTIS_ORIGIN is an http(s) origin");
    let client = Arc::new(
        DeviceClient::new(endpoint.clone(), &application_id)
            .expect("a device client")
            .with_timeout(Duration::from_secs(10)),
    );

    let key = Arc::new(DeviceKeyPair::generate().expect("entropy"));
    let paired = client
        .pair(&code, &key, metadata.clone())
        .await
        .expect("the pairing code is claimed");
    let credentials = Arc::new(CredentialHolder::new(
        Arc::clone(&client),
        paired.device_id.clone(),
        Arc::clone(&key),
    ));
    let delegations = Arc::new(SessionDelegations::new(
        Arc::clone(&client),
        Arc::clone(&credentials),
    ));
    let gateway = Arc::new(
        HttpsGatewayClient::new(
            endpoint,
            &application_id,
            Arc::clone(&delegations) as Arc<dyn DelegationSource>,
        )
        .expect("a gateway client")
        .with_connector(client.connector().clone()),
    );
    let (channel, intents) =
        RemoteChannel::connect(RemoteChannelOptions::new(Arc::clone(&client), credentials));

    let approvals = Arc::new(RemoteApprovals::new());
    let manager = SessionManager::new_with_policy(
        ProviderRouter::new().with(provider),
        gateway,
        Arc::new(ChannelEventSink::new(
            Arc::clone(&channel) as Arc<dyn DeviceChannel>
        )),
        Arc::new(RiskPolicy::v1_defaults()),
        Arc::new(ApprovalManager::new(Arc::clone(&approvals) as Arc<_>)),
    );
    // The Codex session's MCP sockets live beside its neutral workspaces; FakeProvider has no
    // MCP boundary and needs only the workspaces.
    let manager = if codex {
        manager.with_mcp_runtime_dir(workspace_root.path())
    } else {
        manager.with_provider_workspaces(Arc::new(
            ProviderWorkspaces::open(workspace_root.path()).expect("a workspace root"),
        ))
    };
    let sessions = Arc::new(
        manager.with_delegation_issuer(Arc::new(RemoteDelegationIssuer::new(delegations))),
    );
    if codex {
        tokio::spawn(report_postures(Arc::clone(&sessions)));
    }
    let agent = Arc::new(
        RemoteAgent::new(
            sessions,
            Arc::clone(&channel) as Arc<dyn DeviceChannel>,
            approvals,
            paired.user_id.clone(),
            metadata,
        )
        .with_observer(Arc::new(Refusals)),
    );
    tokio::spawn(Arc::clone(&agent).run(intents));

    while !channel.is_connected() {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    println!("READY {}", paired.device_id);

    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("a SIGTERM handler");
    let mut stdin = tokio::io::stdin();
    let mut sink = [0u8; 64];
    tokio::select! {
        _ = sigterm.recv() => {}
        _ = async { while stdin.read(&mut sink).await.is_ok_and(|n| n > 0) {} } => {}
    }
    drop(workspace_root);
}
