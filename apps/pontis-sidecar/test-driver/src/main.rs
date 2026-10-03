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

use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;
use std::time::Duration;

use pontis_identity::key::DeviceKeyPair;
use pontis_protocol::device::DeviceMetadataBody;
use pontis_protocol::errors::TypedError;
use pontis_provider_fake::{ExpectedOutcome, FakeProvider, FakeScript};
use pontis_runtime::{
    ApprovalManager, ChannelEventSink, DelegationSource, HttpsGatewayClient, IntentObserver,
    ProviderRouter, ProviderWorkspaces, RemoteAgent, RemoteApprovals, RemoteDelegationIssuer,
    RiskPolicy, SessionDelegations, SessionManager,
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

#[tokio::main]
async fn main() {
    let origin = env("FORMA_PONTIS_ORIGIN");
    let application_id = env("FORMA_PONTIS_APPLICATION_ID");
    let code = env("FORMA_DRIVER_PAIRING_CODE");
    let version: u64 = env("FORMA_DRIVER_BASE_VERSION")
        .parse()
        .expect("FORMA_DRIVER_BASE_VERSION is a version number");
    let provider = provider(
        &env("FORMA_DRIVER_PROJECT"),
        &env("FORMA_DRIVER_DESIGN"),
        &env("FORMA_DRIVER_LONG_DESIGN"),
        &env("FORMA_DRIVER_LAYER"),
        &env("FORMA_DRIVER_TAGLINE"),
        version,
    );

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

    let workspace_root = tempfile::tempdir().expect("a runtime directory");
    std::fs::set_permissions(
        workspace_root.path(),
        std::fs::Permissions::from_mode(0o700),
    )
    .expect("the runtime directory is owner-only");
    let workspaces =
        Arc::new(ProviderWorkspaces::open(workspace_root.path()).expect("a workspace root"));

    let approvals = Arc::new(RemoteApprovals::new());
    let sessions = Arc::new(
        SessionManager::new_with_policy(
            ProviderRouter::new().with(Arc::new(provider)),
            gateway,
            Arc::new(ChannelEventSink::new(
                Arc::clone(&channel) as Arc<dyn DeviceChannel>
            )),
            Arc::new(RiskPolicy::v1_defaults()),
            Arc::new(ApprovalManager::new(Arc::clone(&approvals) as Arc<_>)),
        )
        .with_provider_workspaces(workspaces)
        .with_delegation_issuer(Arc::new(RemoteDelegationIssuer::new(delegations))),
    );
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
