//! Audit verification checkpoints are local accelerators, never new trust roots.
use super::*;

const PAGE: u64 = 200;

#[derive(Default, Serialize, Deserialize)]
struct Checkpoints {
    count: u64,
    head: Hash,
    pages: Vec<Hash>,
}

fn rows(events: &[AuditEvent]) -> Vec<Json> {
    events.iter().rev().map(|e|json!({"seq":e.seq,"at":e.at_ms,"actor":e.actor.as_ref().map(member_id_text),"proposal":e.proposal,"kind":format!("{:?}",e.kind),"hash":hex::encode(*e.hash)})).collect()
}

/// Verify additions against a freshly certified wallet head; then show a page.
/// An old or inconsistent cache is discarded and verified from genesis again.
pub async fn read<H: Hold>(h: &H, wallet: Principal, before: Option<u64>) -> Result<Json> {
    let view = state(h, wallet).await?;
    let key = format!("wallet-audit:{wallet}");
    let mut cache = h
        .with(|c| {
            Ok(c.workspace
                .private::<Checkpoints>(c.keys()?, &key)?
                .unwrap_or_default())
        })
        .await?;
    if cache.count > view.audit_len
        || (cache.count == view.audit_len && cache.head != view.audit_head)
        || cache.pages.len() as u64 != cache.count / PAGE
    {
        cache = Checkpoints::default();
    }
    let (_, ic) = identity(h).await?;
    let mut last_page = None;
    while cache.count < view.audit_len {
        let end = ((cache.count / PAGE + 1) * PAGE).min(view.audit_len);
        let page: Result<Vec<AuditEvent>> = ic
            .query(wallet, "audit", (cache.count, (end - cache.count) as u16))
            .await?;
        let page = page?;
        ensure(
            page.len() as u64 == end - cache.count,
            Error::IntegrityFailed,
        )?;
        cache.head = verify_audit(cache.head, cache.count, &page)?;
        last_page = Some((cache.count, end, page));
        cache.count = end;
        if end % PAGE == 0 {
            cache.pages.push(cache.head);
        }
    }
    ensure(cache.head == view.audit_head, Error::IntegrityFailed)?;
    h.with(|c| c.workspace.put_private(c.keys()?, &key, &cache))
        .await?;
    let end = before.unwrap_or(view.audit_len).min(view.audit_len);
    let start = end.saturating_sub(1) / PAGE * PAGE;
    let page = if end == 0 {
        vec![]
    } else if let Some((_, _, page)) =
        last_page.filter(|(from, to, _)| *from == start && *to == end)
    {
        page
    } else {
        let page: Result<Vec<AuditEvent>> = ic
            .query(wallet, "audit", (start, (end - start) as u16))
            .await?;
        page?
    };
    let previous = if start == 0 {
        Hash::new([0; 32])
    } else {
        cache.pages[(start / PAGE - 1) as usize]
    };
    let expected = if end == 0 {
        Hash::new([0; 32])
    } else if end == view.audit_len {
        view.audit_head
    } else {
        ensure(end % PAGE == 0, Error::InvalidInput("audit cursor".into()))?;
        cache.pages[(end / PAGE - 1) as usize]
    };
    ensure(
        verify_audit(previous, start, &page)? == expected,
        Error::IntegrityFailed,
    )?;
    Ok(
        json!({"wallet":wallet.to_text(),"verified":true,"events":view.audit_len,"head":hex::encode(*view.audit_head),"read_at":now_ms(),"from":start,"to":end,"previous":hex::encode(*previous),"page_head":hex::encode(*expected),"next":(start>0).then_some(start),"recent":rows(&page)}),
    )
}
