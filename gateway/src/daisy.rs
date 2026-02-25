use axum::{
    body::to_bytes,
    extract::{Extension, Path, Request},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use tower_http::request_id::RequestId;
use url::{form_urlencoded, Url};

use crate::auth::Principal;

const MAX_DAISY_BODY_BYTES: usize = 64 * 1024;
const NOTE_SELECT_FIELDS: &str =
    "id,title,content,tags,pinned,archived,created_at,updated_at,owner_user_id,owner_email";
const SHARE_SELECT_FIELDS: &str =
    "id,note_id,shared_with_user_id,shared_with_email,permission,created_at,updated_at";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum DaisyAccessRole {
    Owner,
    Editor,
    Viewer,
}

impl DaisyAccessRole {
    fn can_edit(self) -> bool {
        matches!(self, Self::Owner | Self::Editor)
    }

    fn can_share(self) -> bool {
        matches!(self, Self::Owner)
    }
}

#[derive(Debug, Clone)]
struct DaisyActorScope {
    user_id: String,
    email: Option<String>,
}

#[derive(Debug, Clone)]
struct DaisyShareTarget {
    shared_with_user_id: Option<String>,
    shared_with_email: Option<String>,
}

#[derive(Debug, Clone)]
struct DaisyResolvedNoteAccess {
    note: DaisyNoteRow,
    role: DaisyAccessRole,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DaisySharePayload {
    permission: Option<String>,
    shared_with_user_id: Option<String>,
    shared_with_email: Option<String>,
    public: Option<bool>,
    public_link: Option<bool>,
    scope: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct DaisyNoteRow {
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    content: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    pinned: bool,
    #[serde(default)]
    archived: bool,
    created_at: Option<String>,
    updated_at: Option<String>,
    owner_user_id: Option<String>,
    owner_email: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct DaisyShareRow {
    id: Option<String>,
    note_id: String,
    shared_with_user_id: Option<String>,
    shared_with_email: Option<String>,
    permission: Option<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct DaisySharePermissionRow {
    note_id: String,
    permission: String,
    created_at: Option<String>,
}

fn request_id_from_extension(request_id: Option<Extension<RequestId>>) -> String {
    request_id
        .as_ref()
        .and_then(|id| id.header_value().to_str().ok())
        .unwrap_or("unknown")
        .to_owned()
}

fn daisy_ok(request_id: &str, status: StatusCode, data: Value) -> Response {
    (
        status,
        Json(json!({
            "ok": true,
            "requestId": request_id,
            "data": data,
            "error": Value::Null,
        })),
    )
        .into_response()
}

fn daisy_error(
    request_id: &str,
    status: StatusCode,
    code: &'static str,
    message: &'static str,
    details: Value,
) -> Response {
    (
        status,
        Json(json!({
            "ok": false,
            "requestId": request_id,
            "data": Value::Null,
            "error": {
                "code": code,
                "message": message,
                "details": details,
            }
        })),
    )
        .into_response()
}

fn data_plane_error(
    request_id: &str,
    code: &'static str,
    message: &'static str,
    details: Value,
) -> Response {
    daisy_error(request_id, StatusCode::BAD_GATEWAY, code, message, details)
}

fn require_principal(
    principal: Option<Extension<Principal>>,
    request_id: &str,
) -> Result<Principal, Response> {
    principal.map(|ext| ext.0).ok_or_else(|| {
        daisy_error(
            request_id,
            StatusCode::UNAUTHORIZED,
            "DAISY_UNAUTHORIZED",
            "Authenticated session required",
            json!({ "authContext": "better-auth-session" }),
        )
    })
}

fn ensure_owner_for_acl(
    role: DaisyAccessRole,
    request_id: &str,
    note_id: &str,
) -> Result<(), Response> {
    if role.can_share() {
        return Ok(());
    }

    Err(daisy_error(
        request_id,
        StatusCode::FORBIDDEN,
        "DAISY_FORBIDDEN",
        "Only note owners can manage ACLs",
        json!({
            "requiredRole": "owner",
            "resolvedRole": role,
            "noteId": note_id,
        }),
    ))
}

fn normalize_permission(value: Option<&str>) -> Option<&'static str> {
    match value.map(|v| v.trim().to_ascii_lowercase()) {
        Some(v) if v == "viewer" => Some("viewer"),
        Some(v) if v == "editor" => Some("editor"),
        _ => None,
    }
}

fn normalize_permission_role(value: Option<&str>) -> Option<DaisyAccessRole> {
    match normalize_permission(value) {
        Some("editor") => Some(DaisyAccessRole::Editor),
        Some("viewer") => Some(DaisyAccessRole::Viewer),
        _ => None,
    }
}

fn normalize_target(payload: &DaisySharePayload) -> Option<DaisyShareTarget> {
    let user_id = payload
        .shared_with_user_id
        .as_ref()
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned);

    let email = payload
        .shared_with_email
        .as_ref()
        .map(|v| v.trim().to_ascii_lowercase())
        .filter(|v| !v.is_empty());

    match (user_id, email) {
        (Some(id), _) => Some(DaisyShareTarget {
            shared_with_user_id: Some(id),
            shared_with_email: None,
        }),
        (_, Some(addr)) => Some(DaisyShareTarget {
            shared_with_user_id: None,
            shared_with_email: Some(addr),
        }),
        _ => None,
    }
}

fn target_as_value(target: &DaisyShareTarget) -> Value {
    json!({
        "sharedWithUserId": target.shared_with_user_id,
        "sharedWithEmail": target.shared_with_email,
    })
}

fn reject_public_link_payload(payload: &DaisySharePayload, request_id: &str) -> Option<Response> {
    let explicit_public = payload.public.unwrap_or(false) || payload.public_link.unwrap_or(false);
    let scope_public = payload
        .scope
        .as_ref()
        .map(|v| v.trim().eq_ignore_ascii_case("public"))
        .unwrap_or(false);

    if explicit_public || scope_public {
        return Some(daisy_error(
            request_id,
            StatusCode::BAD_REQUEST,
            "DAISY_PUBLIC_LINK_DISABLED",
            "Public note links are not supported for internal Daisy API",
            json!({ "noPublicLinks": true }),
        ));
    }

    None
}

async fn parse_json_payload<T: DeserializeOwned>(request: Request) -> Result<T, ()> {
    let bytes = to_bytes(request.into_body(), MAX_DAISY_BODY_BYTES)
        .await
        .map_err(|_| ())?;
    serde_json::from_slice::<T>(&bytes).map_err(|_| ())
}

fn query_param_is_truthy(request: &Request, key: &str) -> bool {
    let Some(query) = request.uri().query() else {
        return false;
    };

    form_urlencoded::parse(query.as_bytes()).any(|(k, v)| {
        if k != key {
            return false;
        }

        matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

fn header_value(headers: &HeaderMap, name: &'static str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn scope_from_headers(principal: &Principal, headers: &HeaderMap) -> DaisyActorScope {
    let user_id = principal.user_id.trim().to_owned();

    let forwarded_user_id = header_value(headers, "x-daisy-session-user-id");
    let forwarded_email =
        header_value(headers, "x-daisy-session-user-email").map(|value| value.to_ascii_lowercase());

    let email = match (forwarded_user_id.as_deref(), forwarded_email) {
        (Some(forwarded_user_id), Some(email)) if forwarded_user_id == user_id => Some(email),
        _ => None,
    };

    DaisyActorScope { user_id, email }
}

fn scope_owns_note(scope: &DaisyActorScope, note: &DaisyNoteRow) -> bool {
    note.owner_user_id
        .as_ref()
        .map(|owner| owner == &scope.user_id)
        .unwrap_or(false)
        || scope
            .email
            .as_ref()
            .zip(note.owner_email.as_ref())
            .map(|(scope_email, owner_email)| scope_email.eq_ignore_ascii_case(owner_email))
            .unwrap_or(false)
}

fn supabase_env(request_id: &str) -> Result<(String, String), Response> {
    let base = std::env::var("SUPABASE_URL")
        .ok()
        .or_else(|| std::env::var("NEXT_PUBLIC_SUPABASE_URL").ok())
        .unwrap_or_default()
        .trim()
        .trim_end_matches('/')
        .to_string();

    let key = std::env::var("SUPABASE_SERVICE_ROLE_KEY")
        .unwrap_or_default()
        .trim()
        .to_string();

    if base.is_empty() || key.is_empty() {
        return Err(daisy_error(
            request_id,
            StatusCode::INTERNAL_SERVER_ERROR,
            "DAISY_CONFIG_ERROR",
            "Supabase env not configured in gateway",
            json!({ "required": ["SUPABASE_URL", "SUPABASE_SERVICE_ROLE_KEY"] }),
        ));
    }

    Ok((base, key))
}

fn supabase_client_with_key(key: &str) -> reqwest::Client {
    let mut default_headers = reqwest::header::HeaderMap::new();
    if let Ok(v) = reqwest::header::HeaderValue::from_str(key) {
        default_headers.insert("apikey", v.clone());
        default_headers.insert(
            reqwest::header::AUTHORIZATION,
            reqwest::header::HeaderValue::from_str(&format!("Bearer {key}")).unwrap_or(v),
        );
    }

    reqwest::Client::builder()
        .default_headers(default_headers)
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

fn table_url(base: &str, table: &str, request_id: &str) -> Result<Url, Response> {
    Url::parse(&format!("{base}/rest/v1/{table}")).map_err(|error| {
        daisy_error(
            request_id,
            StatusCode::INTERNAL_SERVER_ERROR,
            "DAISY_CONFIG_ERROR",
            "Failed to build Daisy data-plane URL",
            json!({ "table": table, "reason": error.to_string() }),
        )
    })
}

fn supabase_error_message(raw_body: &str) -> String {
    if raw_body.trim().is_empty() {
        return "empty upstream response".to_string();
    }

    if let Ok(value) = serde_json::from_str::<Value>(raw_body) {
        if let Some(message) = value.get("message").and_then(Value::as_str) {
            return message.to_string();
        }
        if let Some(message) = value.get("error").and_then(Value::as_str) {
            return message.to_string();
        }
        if let Some(message) = value.get("details").and_then(Value::as_str) {
            return message.to_string();
        }
        if let Some(message) = value
            .get("error")
            .and_then(Value::as_object)
            .and_then(|error| error.get("message"))
            .and_then(Value::as_str)
        {
            return message.to_string();
        }
    }

    raw_body.chars().take(240).collect()
}

async fn supabase_request_text(
    request_id: &str,
    context: &'static str,
    request: reqwest::RequestBuilder,
) -> Result<(StatusCode, String), Response> {
    let response = request.send().await.map_err(|error| {
        data_plane_error(
            request_id,
            "DAISY_DATA_PLANE_UNAVAILABLE",
            "Unable to reach Daisy data plane",
            json!({ "context": context, "reason": error.to_string() }),
        )
    })?;

    let status =
        StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let body = response.text().await.unwrap_or_default();
    Ok((status, body))
}

fn parse_json_rows<T: DeserializeOwned>(
    request_id: &str,
    context: &'static str,
    raw_body: &str,
) -> Result<Vec<T>, Response> {
    if raw_body.trim().is_empty() {
        return Ok(Vec::new());
    }

    serde_json::from_str::<Vec<T>>(raw_body).map_err(|error| {
        data_plane_error(
            request_id,
            "DAISY_DATA_PLANE_ERROR",
            "Daisy data plane returned invalid JSON",
            json!({
                "context": context,
                "reason": error.to_string(),
            }),
        )
    })
}

async fn supabase_get_rows<T: DeserializeOwned>(
    client: &reqwest::Client,
    url: Url,
    request_id: &str,
    context: &'static str,
) -> Result<Vec<T>, Response> {
    let (status, body) =
        supabase_request_text(request_id, context, client.get(url.as_str())).await?;

    if !status.is_success() {
        return Err(data_plane_error(
            request_id,
            "DAISY_DATA_PLANE_ERROR",
            "Daisy data plane query failed",
            json!({
                "context": context,
                "upstreamStatus": status.as_u16(),
                "upstreamError": supabase_error_message(&body),
            }),
        ));
    }

    parse_json_rows(request_id, context, &body)
}

fn add_archived_filter(url: &mut Url, include_archived: bool) {
    if include_archived {
        return;
    }

    url.query_pairs_mut().append_pair("archived", "eq.false");
}

async fn fetch_notes_for_owner(
    client: &reqwest::Client,
    base: &str,
    scope: &DaisyActorScope,
    include_archived: bool,
    request_id: &str,
) -> Result<Vec<DaisyNoteRow>, Response> {
    let mut rows = Vec::new();

    if !scope.user_id.is_empty() {
        let mut by_user = table_url(base, "daisy_notes", request_id)?;
        {
            let mut qp = by_user.query_pairs_mut();
            qp.append_pair("select", NOTE_SELECT_FIELDS);
            qp.append_pair("owner_user_id", &format!("eq.{}", scope.user_id));
            qp.append_pair("order", "updated_at.desc");
        }
        add_archived_filter(&mut by_user, include_archived);
        rows.extend(
            supabase_get_rows::<DaisyNoteRow>(
                client,
                by_user,
                request_id,
                "list_owner_notes_by_user",
            )
            .await?,
        );
    }

    if let Some(email) = scope.email.as_deref() {
        let mut by_email = table_url(base, "daisy_notes", request_id)?;
        {
            let mut qp = by_email.query_pairs_mut();
            qp.append_pair("select", NOTE_SELECT_FIELDS);
            qp.append_pair("owner_email", &format!("eq.{email}"));
            qp.append_pair("order", "updated_at.desc");
        }
        add_archived_filter(&mut by_email, include_archived);
        rows.extend(
            supabase_get_rows::<DaisyNoteRow>(
                client,
                by_email,
                request_id,
                "list_owner_notes_by_email",
            )
            .await?,
        );
    }

    let mut deduped = HashMap::new();
    for row in rows {
        deduped.entry(row.id.clone()).or_insert(row);
    }

    Ok(deduped.into_values().collect())
}

async fn fetch_share_permissions_by_target(
    client: &reqwest::Client,
    base: &str,
    field: &'static str,
    value: &str,
    request_id: &str,
    context: &'static str,
) -> Result<Vec<DaisySharePermissionRow>, Response> {
    let mut url = table_url(base, "daisy_note_shares", request_id)?;
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("select", "note_id,permission,created_at");
        qp.append_pair(field, &format!("eq.{value}"));
        qp.append_pair("order", "created_at.desc");
    }

    supabase_get_rows::<DaisySharePermissionRow>(client, url, request_id, context).await
}

async fn fetch_shared_permissions_for_scope(
    client: &reqwest::Client,
    base: &str,
    scope: &DaisyActorScope,
    request_id: &str,
) -> Result<HashMap<String, DaisyAccessRole>, Response> {
    let mut entries = Vec::new();

    if !scope.user_id.is_empty() {
        entries.extend(
            fetch_share_permissions_by_target(
                client,
                base,
                "shared_with_user_id",
                &scope.user_id,
                request_id,
                "list_share_permissions_by_user",
            )
            .await?,
        );
    }

    if let Some(email) = scope.email.as_deref() {
        entries.extend(
            fetch_share_permissions_by_target(
                client,
                base,
                "shared_with_email",
                email,
                request_id,
                "list_share_permissions_by_email",
            )
            .await?,
        );
    }

    entries.sort_by(|left, right| right.created_at.cmp(&left.created_at));

    let mut map = HashMap::new();
    for entry in entries {
        if map.contains_key(&entry.note_id) {
            continue;
        }
        if let Some(role) = normalize_permission_role(Some(&entry.permission)) {
            map.insert(entry.note_id, role);
        }
    }

    Ok(map)
}

async fn fetch_notes_by_ids(
    client: &reqwest::Client,
    base: &str,
    ids: &[String],
    include_archived: bool,
    request_id: &str,
) -> Result<Vec<DaisyNoteRow>, Response> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    let mut url = table_url(base, "daisy_notes", request_id)?;
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("select", NOTE_SELECT_FIELDS);
        qp.append_pair("id", &format!("in.({})", ids.join(",")));
        qp.append_pair("order", "updated_at.desc");
    }
    add_archived_filter(&mut url, include_archived);

    supabase_get_rows::<DaisyNoteRow>(client, url, request_id, "list_shared_notes").await
}

async fn fetch_note_by_id(
    client: &reqwest::Client,
    base: &str,
    note_id: &str,
    request_id: &str,
) -> Result<Option<DaisyNoteRow>, Response> {
    let mut url = table_url(base, "daisy_notes", request_id)?;
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("select", NOTE_SELECT_FIELDS);
        qp.append_pair("id", &format!("eq.{note_id}"));
        qp.append_pair("limit", "1");
    }

    let rows = supabase_get_rows::<DaisyNoteRow>(client, url, request_id, "get_note_by_id").await?;
    Ok(rows.into_iter().next())
}

async fn fetch_note_share_role_by_target(
    client: &reqwest::Client,
    base: &str,
    note_id: &str,
    field: &'static str,
    value: &str,
    request_id: &str,
    context: &'static str,
) -> Result<Option<DaisySharePermissionRow>, Response> {
    let mut url = table_url(base, "daisy_note_shares", request_id)?;
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("select", "note_id,permission,created_at");
        qp.append_pair("note_id", &format!("eq.{note_id}"));
        qp.append_pair(field, &format!("eq.{value}"));
        qp.append_pair("order", "created_at.desc");
        qp.append_pair("limit", "1");
    }

    let rows =
        supabase_get_rows::<DaisySharePermissionRow>(client, url, request_id, context).await?;
    Ok(rows.into_iter().next())
}

async fn fetch_direct_share_role_for_note(
    client: &reqwest::Client,
    base: &str,
    scope: &DaisyActorScope,
    note_id: &str,
    request_id: &str,
) -> Result<Option<DaisyAccessRole>, Response> {
    let mut candidates = Vec::new();

    if !scope.user_id.is_empty() {
        if let Some(entry) = fetch_note_share_role_by_target(
            client,
            base,
            note_id,
            "shared_with_user_id",
            &scope.user_id,
            request_id,
            "resolve_note_access_by_user",
        )
        .await?
        {
            candidates.push(entry);
        }
    }

    if let Some(email) = scope.email.as_deref() {
        if let Some(entry) = fetch_note_share_role_by_target(
            client,
            base,
            note_id,
            "shared_with_email",
            email,
            request_id,
            "resolve_note_access_by_email",
        )
        .await?
        {
            candidates.push(entry);
        }
    }

    candidates.sort_by(|left, right| right.created_at.cmp(&left.created_at));

    for candidate in candidates {
        if let Some(role) = normalize_permission_role(Some(&candidate.permission)) {
            return Ok(Some(role));
        }
    }

    Ok(None)
}

async fn resolve_note_access(
    client: &reqwest::Client,
    base: &str,
    scope: &DaisyActorScope,
    note_id: &str,
    request_id: &str,
) -> Result<Option<DaisyResolvedNoteAccess>, Response> {
    let Some(note) = fetch_note_by_id(client, base, note_id, request_id).await? else {
        return Ok(None);
    };

    if scope_owns_note(scope, &note) {
        return Ok(Some(DaisyResolvedNoteAccess {
            note,
            role: DaisyAccessRole::Owner,
        }));
    }

    if let Some(role) =
        fetch_direct_share_role_for_note(client, base, scope, note_id, request_id).await?
    {
        return Ok(Some(DaisyResolvedNoteAccess { note, role }));
    }

    Ok(None)
}

fn note_row_to_response(note: &DaisyNoteRow, role: DaisyAccessRole) -> Value {
    json!({
        "id": note.id,
        "title": note.title,
        "content": note.content,
        "tags": note.tags,
        "pinned": note.pinned,
        "archived": note.archived,
        "createdAt": note.created_at,
        "updatedAt": note.updated_at,
        "access": {
            "role": role,
            "canEdit": role.can_edit(),
            "canShare": role.can_share(),
        }
    })
}

fn share_row_to_response(share: DaisyShareRow) -> Value {
    json!({
        "id": share.id,
        "noteId": share.note_id,
        "sharedWithUserId": share.shared_with_user_id,
        "sharedWithEmail": share.shared_with_email,
        "permission": normalize_permission(share.permission.as_deref()),
        "createdAt": share.created_at,
        "updatedAt": share.updated_at,
    })
}

async fn fetch_note_shares(
    client: &reqwest::Client,
    base: &str,
    note_id: &str,
    request_id: &str,
) -> Result<Vec<DaisyShareRow>, Response> {
    let mut url = table_url(base, "daisy_note_shares", request_id)?;
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("select", SHARE_SELECT_FIELDS);
        qp.append_pair("note_id", &format!("eq.{note_id}"));
        qp.append_pair("order", "created_at.desc");
    }

    supabase_get_rows::<DaisyShareRow>(client, url, request_id, "list_note_shares").await
}

async fn upsert_note_share_in_data_plane(
    client: &reqwest::Client,
    base: &str,
    note_id: &str,
    target: &DaisyShareTarget,
    permission: &str,
    scope: &DaisyActorScope,
    request_id: &str,
) -> Result<DaisyShareRow, Response> {
    let conflict_target = if target.shared_with_user_id.is_some() {
        "note_id,shared_with_user_id"
    } else {
        "note_id,shared_with_email"
    };

    let mut url = table_url(base, "daisy_note_shares", request_id)?;
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("on_conflict", conflict_target);
        qp.append_pair("select", SHARE_SELECT_FIELDS);
    }

    let payload = json!({
        "note_id": note_id,
        "shared_with_user_id": target.shared_with_user_id,
        "shared_with_email": target.shared_with_email,
        "permission": permission,
        "shared_by_user_id": scope.user_id,
        "shared_by_email": scope.email,
    });

    let (status, body) = supabase_request_text(
        request_id,
        "upsert_note_share",
        client
            .post(url.as_str())
            .header(
                "Prefer",
                "resolution=merge-duplicates,return=representation",
            )
            .json(&payload),
    )
    .await?;

    if !status.is_success() {
        return Err(data_plane_error(
            request_id,
            "DAISY_DATA_PLANE_ERROR",
            "Daisy data plane share upsert failed",
            json!({
                "upstreamStatus": status.as_u16(),
                "upstreamError": supabase_error_message(&body),
            }),
        ));
    }

    let rows = parse_json_rows::<DaisyShareRow>(request_id, "upsert_note_share", &body)?;
    rows.into_iter().next().ok_or_else(|| {
        data_plane_error(
            request_id,
            "DAISY_DATA_PLANE_ERROR",
            "Daisy data plane share upsert returned no rows",
            json!({}),
        )
    })
}

async fn remove_note_share_in_data_plane(
    client: &reqwest::Client,
    base: &str,
    note_id: &str,
    target: &DaisyShareTarget,
    request_id: &str,
) -> Result<(), Response> {
    let mut url = table_url(base, "daisy_note_shares", request_id)?;
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("note_id", &format!("eq.{note_id}"));
        if let Some(shared_with_user_id) = target.shared_with_user_id.as_deref() {
            qp.append_pair("shared_with_user_id", &format!("eq.{shared_with_user_id}"));
        }
        if let Some(shared_with_email) = target.shared_with_email.as_deref() {
            qp.append_pair("shared_with_email", &format!("eq.{shared_with_email}"));
        }
    }

    let (status, body) = supabase_request_text(
        request_id,
        "remove_note_share",
        client
            .delete(url.as_str())
            .header("Prefer", "return=minimal"),
    )
    .await?;

    if !status.is_success() {
        return Err(data_plane_error(
            request_id,
            "DAISY_DATA_PLANE_ERROR",
            "Daisy data plane share removal failed",
            json!({
                "upstreamStatus": status.as_u16(),
                "upstreamError": supabase_error_message(&body),
            }),
        ));
    }

    Ok(())
}

fn note_not_found(request_id: &str, note_id: &str) -> Response {
    daisy_error(
        request_id,
        StatusCode::NOT_FOUND,
        "DAISY_NOTE_NOT_FOUND",
        "Note not found",
        json!({ "noteId": note_id }),
    )
}

pub async fn list_notes(
    principal: Option<Extension<Principal>>,
    request_id: Option<Extension<RequestId>>,
    request: Request,
) -> Response {
    let request_id = request_id_from_extension(request_id);
    let principal = match require_principal(principal, &request_id) {
        Ok(principal) => principal,
        Err(response) => return response,
    };

    let include_archived = query_param_is_truthy(&request, "includeArchived");
    let scope = scope_from_headers(&principal, request.headers());

    let (base, key) = match supabase_env(&request_id) {
        Ok(config) => config,
        Err(response) => return response,
    };

    let client = supabase_client_with_key(&key);

    let owner_notes =
        match fetch_notes_for_owner(&client, &base, &scope, include_archived, &request_id).await {
            Ok(notes) => notes,
            Err(response) => return response,
        };

    let shared_permissions =
        match fetch_shared_permissions_for_scope(&client, &base, &scope, &request_id).await {
            Ok(permissions) => permissions,
            Err(response) => return response,
        };

    let shared_note_ids = shared_permissions.keys().cloned().collect::<Vec<_>>();
    let shared_notes = match fetch_notes_by_ids(
        &client,
        &base,
        &shared_note_ids,
        include_archived,
        &request_id,
    )
    .await
    {
        Ok(notes) => notes,
        Err(response) => return response,
    };

    let mut by_id = HashMap::new();
    for note in owner_notes.into_iter().chain(shared_notes) {
        by_id.entry(note.id.clone()).or_insert(note);
    }

    let mut notes = by_id
        .into_values()
        .filter_map(|note| {
            let role = if scope_owns_note(&scope, &note) {
                Some(DaisyAccessRole::Owner)
            } else {
                shared_permissions.get(&note.id).copied()
            }?;

            Some((
                note.updated_at.clone().unwrap_or_default(),
                note_row_to_response(&note, role),
            ))
        })
        .collect::<Vec<_>>();

    notes.sort_by(|left, right| right.0.cmp(&left.0));

    daisy_ok(
        &request_id,
        StatusCode::OK,
        json!({
            "notes": notes.into_iter().map(|(_, note)| note).collect::<Vec<_>>()
        }),
    )
}

pub async fn get_note_by_id(
    Path(note_id): Path<String>,
    principal: Option<Extension<Principal>>,
    request_id: Option<Extension<RequestId>>,
    request: Request,
) -> Response {
    let request_id = request_id_from_extension(request_id);
    let principal = match require_principal(principal, &request_id) {
        Ok(principal) => principal,
        Err(response) => return response,
    };

    let note_id = note_id.trim().to_owned();
    if note_id.is_empty() {
        return note_not_found(&request_id, "");
    }

    let scope = scope_from_headers(&principal, request.headers());

    let (base, key) = match supabase_env(&request_id) {
        Ok(config) => config,
        Err(response) => return response,
    };

    let client = supabase_client_with_key(&key);
    let access = match resolve_note_access(&client, &base, &scope, &note_id, &request_id).await {
        Ok(access) => access,
        Err(response) => return response,
    };

    let Some(access) = access else {
        return note_not_found(&request_id, &note_id);
    };

    daisy_ok(
        &request_id,
        StatusCode::OK,
        json!({
            "note": note_row_to_response(&access.note, access.role)
        }),
    )
}

pub async fn list_note_shares(
    Path(note_id): Path<String>,
    principal: Option<Extension<Principal>>,
    request_id: Option<Extension<RequestId>>,
    request: Request,
) -> Response {
    let request_id = request_id_from_extension(request_id);
    let principal = match require_principal(principal, &request_id) {
        Ok(principal) => principal,
        Err(response) => return response,
    };

    let note_id = note_id.trim().to_owned();
    if note_id.is_empty() {
        return note_not_found(&request_id, "");
    }

    let scope = scope_from_headers(&principal, request.headers());

    let (base, key) = match supabase_env(&request_id) {
        Ok(config) => config,
        Err(response) => return response,
    };

    let client = supabase_client_with_key(&key);
    let access = match resolve_note_access(&client, &base, &scope, &note_id, &request_id).await {
        Ok(access) => access,
        Err(response) => return response,
    };

    let Some(access) = access else {
        return note_not_found(&request_id, &note_id);
    };

    if let Err(response) = ensure_owner_for_acl(access.role, &request_id, &note_id) {
        return response;
    }

    let shares = match fetch_note_shares(&client, &base, &note_id, &request_id).await {
        Ok(shares) => shares,
        Err(response) => return response,
    };

    daisy_ok(
        &request_id,
        StatusCode::OK,
        json!({
            "shares": shares.into_iter().map(share_row_to_response).collect::<Vec<_>>(),
        }),
    )
}

pub async fn upsert_note_share(
    Path(note_id): Path<String>,
    principal: Option<Extension<Principal>>,
    request_id: Option<Extension<RequestId>>,
    request: Request,
) -> Response {
    let request_id = request_id_from_extension(request_id);
    let principal = match require_principal(principal, &request_id) {
        Ok(principal) => principal,
        Err(response) => return response,
    };

    let note_id = note_id.trim().to_owned();
    if note_id.is_empty() {
        return note_not_found(&request_id, "");
    }

    let scope = scope_from_headers(&principal, request.headers());

    let payload = match parse_json_payload::<DaisySharePayload>(request).await {
        Ok(payload) => payload,
        Err(_) => {
            return daisy_error(
                &request_id,
                StatusCode::BAD_REQUEST,
                "DAISY_INVALID_SHARE_PAYLOAD",
                "Share payload must be valid JSON object",
                json!({ "expected": ["permission", "sharedWithUserId|sharedWithEmail"] }),
            );
        }
    };

    if let Some(response) = reject_public_link_payload(&payload, &request_id) {
        return response;
    }

    let permission = match normalize_permission(payload.permission.as_deref()) {
        Some(permission) => permission,
        None => {
            return daisy_error(
                &request_id,
                StatusCode::BAD_REQUEST,
                "DAISY_INVALID_SHARE_PAYLOAD",
                "permission must be viewer or editor",
                json!({ "permission": payload.permission }),
            );
        }
    };

    let target = match normalize_target(&payload) {
        Some(target) => target,
        None => {
            return daisy_error(
                &request_id,
                StatusCode::BAD_REQUEST,
                "DAISY_INVALID_SHARE_PAYLOAD",
                "sharedWithUserId or sharedWithEmail is required",
                json!({ "target": "missing" }),
            );
        }
    };

    let (base, key) = match supabase_env(&request_id) {
        Ok(config) => config,
        Err(response) => return response,
    };

    let client = supabase_client_with_key(&key);
    let access = match resolve_note_access(&client, &base, &scope, &note_id, &request_id).await {
        Ok(access) => access,
        Err(response) => return response,
    };

    let Some(access) = access else {
        return note_not_found(&request_id, &note_id);
    };

    if let Err(response) = ensure_owner_for_acl(access.role, &request_id, &note_id) {
        return response;
    }

    let target_is_owner = target
        .shared_with_user_id
        .as_ref()
        .zip(access.note.owner_user_id.as_ref())
        .map(|(target_user_id, owner_user_id)| target_user_id == owner_user_id)
        .unwrap_or(false)
        || target
            .shared_with_email
            .as_ref()
            .zip(access.note.owner_email.as_ref())
            .map(|(target_email, owner_email)| target_email.eq_ignore_ascii_case(owner_email))
            .unwrap_or(false);

    if target_is_owner {
        return daisy_error(
            &request_id,
            StatusCode::BAD_REQUEST,
            "DAISY_INVALID_SHARE_PAYLOAD",
            "Note owner already has access",
            json!({ "target": target_as_value(&target) }),
        );
    }

    let share = match upsert_note_share_in_data_plane(
        &client,
        &base,
        &note_id,
        &target,
        permission,
        &scope,
        &request_id,
    )
    .await
    {
        Ok(share) => share,
        Err(response) => return response,
    };

    daisy_ok(
        &request_id,
        StatusCode::OK,
        json!({
            "share": share_row_to_response(share),
        }),
    )
}

pub async fn remove_note_share(
    Path(note_id): Path<String>,
    principal: Option<Extension<Principal>>,
    request_id: Option<Extension<RequestId>>,
    request: Request,
) -> Response {
    let request_id = request_id_from_extension(request_id);
    let principal = match require_principal(principal, &request_id) {
        Ok(principal) => principal,
        Err(response) => return response,
    };

    let note_id = note_id.trim().to_owned();
    if note_id.is_empty() {
        return note_not_found(&request_id, "");
    }

    let scope = scope_from_headers(&principal, request.headers());

    let payload = match parse_json_payload::<DaisySharePayload>(request).await {
        Ok(payload) => payload,
        Err(_) => {
            return daisy_error(
                &request_id,
                StatusCode::BAD_REQUEST,
                "DAISY_INVALID_SHARE_PAYLOAD",
                "Share removal payload must be valid JSON object",
                json!({ "expected": ["sharedWithUserId|sharedWithEmail"] }),
            );
        }
    };

    let target = match normalize_target(&payload) {
        Some(target) => target,
        None => {
            return daisy_error(
                &request_id,
                StatusCode::BAD_REQUEST,
                "DAISY_INVALID_SHARE_PAYLOAD",
                "sharedWithUserId or sharedWithEmail is required",
                json!({ "target": "missing" }),
            );
        }
    };

    let (base, key) = match supabase_env(&request_id) {
        Ok(config) => config,
        Err(response) => return response,
    };

    let client = supabase_client_with_key(&key);
    let access = match resolve_note_access(&client, &base, &scope, &note_id, &request_id).await {
        Ok(access) => access,
        Err(response) => return response,
    };

    let Some(access) = access else {
        return note_not_found(&request_id, &note_id);
    };

    if let Err(response) = ensure_owner_for_acl(access.role, &request_id, &note_id) {
        return response;
    }

    if let Err(response) =
        remove_note_share_in_data_plane(&client, &base, &note_id, &target, &request_id).await
    {
        return response;
    }

    daisy_ok(
        &request_id,
        StatusCode::OK,
        json!({
            "removed": true,
            "noteId": note_id,
            "target": target_as_value(&target),
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::AuthMethod;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        middleware::{self as axum_mw, Next},
        routing::get,
        Router,
    };
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    async fn body_json(resp: Response) -> serde_json::Value {
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    fn principal_with_roles(roles: &[&str]) -> Principal {
        Principal {
            user_id: "user_daisy_test".to_string(),
            org_id: None,
            roles: roles.iter().map(|role| (*role).to_string()).collect(),
            session_id: "session_daisy_test".to_string(),
            auth_method: AuthMethod::Bearer,
        }
    }

    fn build_daisy_contract_router(principal: Option<Principal>) -> Router {
        let mut router = Router::new()
            .route("/api/daisy-notes/notes", get(list_notes))
            .route("/api/daisy-notes/notes/{note_id}", get(get_note_by_id))
            .route(
                "/api/daisy-notes/notes/{note_id}/share",
                get(list_note_shares)
                    .post(upsert_note_share)
                    .delete(remove_note_share),
            );

        if let Some(principal) = principal {
            router = router.layer(axum_mw::from_fn(
                move |mut req: Request<Body>, next: Next| {
                    let principal = principal.clone();
                    async move {
                        req.extensions_mut().insert(principal);
                        next.run(req).await
                    }
                },
            ));
        }

        router
    }

    #[tokio::test]
    async fn daisy_routes_require_authenticated_principal() {
        let router = build_daisy_contract_router(None);

        let resp = router
            .oneshot(
                Request::builder()
                    .uri("/api/daisy-notes/notes")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        let body = body_json(resp).await;
        assert_eq!(body["ok"], false);
        assert_eq!(body["error"]["code"], "DAISY_UNAUTHORIZED");
    }

    #[tokio::test]
    async fn daisy_share_routes_reject_public_link_payloads_before_data_plane_lookup() {
        let router = build_daisy_contract_router(Some(principal_with_roles(&["owner"])));

        let no_public = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/daisy-notes/notes/note_1/share")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"permission":"viewer","sharedWithUserId":"user_2","public":true}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(no_public.status(), StatusCode::BAD_REQUEST);
        let no_public_body = body_json(no_public).await;
        assert_eq!(
            no_public_body["error"]["code"],
            "DAISY_PUBLIC_LINK_DISABLED"
        );
    }

    #[test]
    fn ensure_owner_for_acl_rejects_non_owner_roles() {
        let forbidden = ensure_owner_for_acl(DaisyAccessRole::Viewer, "req_1", "note_1")
            .expect_err("viewer should be forbidden");
        assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);

        let allowed = ensure_owner_for_acl(DaisyAccessRole::Owner, "req_2", "note_2");
        assert!(allowed.is_ok());
    }

    #[test]
    fn normalize_target_prefers_user_id_over_email() {
        let payload = DaisySharePayload {
            permission: Some("viewer".to_string()),
            shared_with_user_id: Some("  user_123  ".to_string()),
            shared_with_email: Some("SomeOne@GreenHatSec.com".to_string()),
            public: None,
            public_link: None,
            scope: None,
        };

        let target = normalize_target(&payload).expect("target expected");
        assert_eq!(target.shared_with_user_id.as_deref(), Some("user_123"));
        assert_eq!(target.shared_with_email, None);
    }

    #[test]
    fn scope_owns_note_by_user_or_email() {
        let note = DaisyNoteRow {
            id: "note_1".to_string(),
            title: "t".to_string(),
            content: "c".to_string(),
            tags: vec![],
            pinned: false,
            archived: false,
            created_at: None,
            updated_at: None,
            owner_user_id: Some("user_owner".to_string()),
            owner_email: Some("owner@greenhatsec.com".to_string()),
        };

        let by_user = DaisyActorScope {
            user_id: "user_owner".to_string(),
            email: None,
        };
        assert!(scope_owns_note(&by_user, &note));

        let by_email = DaisyActorScope {
            user_id: "someone_else".to_string(),
            email: Some("owner@greenhatsec.com".to_string()),
        };
        assert!(scope_owns_note(&by_email, &note));

        let other = DaisyActorScope {
            user_id: "someone_else".to_string(),
            email: Some("other@greenhatsec.com".to_string()),
        };
        assert!(!scope_owns_note(&other, &note));
    }
}
