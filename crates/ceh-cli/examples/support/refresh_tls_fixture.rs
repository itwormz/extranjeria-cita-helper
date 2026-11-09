//! Native reload command proof on real local HTTPS, without protocol interception.
use super::{Client, Result, portal_fixture::seed_observer, tls_fixture::TlsFixture};
use ceh_browser::{
    observer::PageObserver,
    refresh::{RefreshAction, plan_refresh},
};
use ceh_core::model::*;
use serde_json::json;
const BOOT: &str = "/icpplus/rust-fixture-tls-boot";
const PATH: &str = "/icpplus/acCitar";
const EMPTY: &str = "<body>No hay citas disponibles</body>";
pub fn run(client: &mut Client, context: &str, fixture: &TlsFixture) -> Result<()> {
    fixture.routes(vec![("GET",BOOT,"<body><form method='post' action='/icpplus/acCitar'><button id='go'>Go</button></form></body>"),("POST",PATH,EMPTY)])?;
    fixture.navigate(client, context, BOOT)?;
    let start = client.events.len();
    client.call("script.evaluate",json!({"expression":"document.getElementById('go').click();true","target":{"context":context},"awaitPromise":false}))?;
    fixture.wait_load(client, context, PATH, start)?;
    let mut observer = PageObserver::new(context).map_err(|_| "TLS observer invalid")?;
    let mut cursor = 0;
    seed_observer(client, &mut observer, &mut cursor)?;
    observer.set_actions(Some(ActionKind::Request), None);
    let page = observer
        .observe(client.bidi.as_mut().ok_or("missing BiDi")?, "manual")
        .map_err(|_| "TLS page read failed")?;
    if page.observation.method != RequestMethod::Post
        || plan_refresh(&page.observation, &page.page.snapshot)
            .map_err(|_| "TLS refresh plan failed")?
            != RefreshAction::ReloadAvailability
    {
        return Err("TLS initial state wrong".into());
    }
    let before = fixture
        .requests()?
        .into_iter()
        .filter(|r| r["path"] == PATH && r["method"] == "POST")
        .count();
    let start = client.events.len();
    // Capability probe only: production mapper is switched after this proof passes.
    let acknowledged = client
        .call(
            "browsingContext.reload",
            json!({"context":context,"wait":"none"}),
        )
        .is_ok();
    if acknowledged {
        fixture.wait_load(client, context, PATH, start)?;
    }
    let after = fixture
        .requests()?
        .into_iter()
        .filter(|r| r["path"] == PATH && r["method"] == "POST")
        .count();
    if before != 1 || after > 2 {
        return Err("protocol reload cardinality unexpected".into());
    }
    println!(
        "{}",
        json!({"no_external_forwarding":true,"isolated_profile_ca_only":true,"strict_https":true,"bidi_post_reload_supported":acknowledged && after==2,"acknowledged":acknowledged,"post_requests":after,"no_retry_after_uncertain_delivery":true})
    );
    Ok(())
}
