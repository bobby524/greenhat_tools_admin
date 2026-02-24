use axum::{
    body::to_bytes,
    extract::{Extension, Path, Request},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use tower_http::request_id::RequestId;

use crate::auth::Principal;

const MAX_DAISY_BODY_BYTES: usize = 64 * 1024;

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

fn role_from_principal(principal: &Principal) -> DaisyAccessRole {
    let roles: Vec<String> = principal
        .roles
        .iter()
        .map(|r| r.trim().to_ascii_lowercase())
        .collect();

    if roles.iter().any(|r| r == "owner" || r == "admin") {
        DaisyAccessRole::Owner
    } else if roles.iter().any(|r| r == "editor") {
        DaisyAccessRole::Editor
    } else {
        DaisyAccessRole::Viewer
    }
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

fn normalize_target(payload: &DaisySharePayload) -> Option<Value> {
    let user_id = payload
        .shared_with_user_id
        .as_ref()
        .map(|v| v.trim())
        .filter(|v| !v.is_empty());
    let email = payload
        .shared_with_email
        .as_ref()
        .map(|v| v.trim().to_ascii_lowercase())
        .filter(|v| !v.is_empty());

    match (user_id, email) {
        (Some(id), _) => Some(json!({ "sharedWithUserId": id, "sharedWithEmail": Value::Null })),
        (_, Some(addr)) => {
            Some(json!({ "sharedWithUserId": Value::Null, "sharedWithEmail": addr }))
        }
        _ => None,
    }
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

fn sample_note(note_id: &str, role: DaisyAccessRole) -> Value {
    json!({
        "id": note_id,
        "title": "Daisy Gateway Scaffold",
        "content": "Phase 2 gateway scaffold placeholder",
        "tags": ["gateway", "scaffold"],
        "pinned": false,
        "archived": false,
        "createdAt": "2026-02-24T00:00:00.000Z",
        "updatedAt": "2026-02-24T00:00:00.000Z",
        "access": {
            "role": role,
            "canEdit": role.can_edit(),
            "canShare": role.can_share(),
        }
    })
}

pub async fn list_notes(
    principal: Option<Extension<Principal>>,
    request_id: Option<Extension<RequestId>>,
) -> Response {
    let request_id = request_id_from_extension(request_id);
    let principal = match require_principal(principal, &request_id) {
        Ok(principal) => principal,
        Err(response) => return response,
    };

    let role = role_from_principal(&principal);
    let notes = vec![sample_note("note_scaffold_1", role)];

    daisy_ok(
        &request_id,
        StatusCode::OK,
        json!({
            "notes": notes,
            "scaffold": true,
        }),
    )
}

pub async fn get_note_by_id(
    Path(note_id): Path<String>,
    principal: Option<Extension<Principal>>,
    request_id: Option<Extension<RequestId>>,
) -> Response {
    let request_id = request_id_from_extension(request_id);
    let principal = match require_principal(principal, &request_id) {
        Ok(principal) => principal,
        Err(response) => return response,
    };

    if note_id.trim().is_empty() {
        return daisy_error(
            &request_id,
            StatusCode::NOT_FOUND,
            "DAISY_NOTE_NOT_FOUND",
            "Note not found",
            json!({ "noteId": note_id }),
        );
    }

    let role = role_from_principal(&principal);
    daisy_ok(
        &request_id,
        StatusCode::OK,
        json!({
            "note": sample_note(&note_id, role),
            "scaffold": true,
        }),
    )
}

pub async fn list_note_shares(
    Path(note_id): Path<String>,
    principal: Option<Extension<Principal>>,
    request_id: Option<Extension<RequestId>>,
) -> Response {
    let request_id = request_id_from_extension(request_id);
    let principal = match require_principal(principal, &request_id) {
        Ok(principal) => principal,
        Err(response) => return response,
    };

    let role = role_from_principal(&principal);
    if let Err(response) = ensure_owner_for_acl(role, &request_id, &note_id) {
        return response;
    }

    daisy_ok(
        &request_id,
        StatusCode::OK,
        json!({
            "shares": [
                {
                    "id": "share_scaffold_1",
                    "noteId": note_id,
                    "sharedWithUserId": "user_viewer_scaffold",
                    "sharedWithEmail": Value::Null,
                    "permission": "viewer",
                    "createdAt": "2026-02-24T00:00:00.000Z",
                    "updatedAt": "2026-02-24T00:00:00.000Z"
                }
            ],
            "scaffold": true,
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

    let role = role_from_principal(&principal);
    if let Err(response) = ensure_owner_for_acl(role, &request_id, &note_id) {
        return response;
    }

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

    daisy_ok(
        &request_id,
        StatusCode::OK,
        json!({
            "share": {
                "id": "share_scaffold_new",
                "noteId": note_id,
                "permission": permission,
                "sharedWithUserId": target["sharedWithUserId"],
                "sharedWithEmail": target["sharedWithEmail"],
                "createdAt": "2026-02-24T00:00:00.000Z",
                "updatedAt": "2026-02-24T00:00:00.000Z"
            },
            "scaffold": true,
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

    let role = role_from_principal(&principal);
    if let Err(response) = ensure_owner_for_acl(role, &request_id, &note_id) {
        return response;
    }

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

    daisy_ok(
        &request_id,
        StatusCode::OK,
        json!({
            "removed": true,
            "noteId": note_id,
            "target": target,
            "scaffold": true,
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
    async fn daisy_share_routes_forbid_non_owner_acl_management() {
        let router = build_daisy_contract_router(Some(principal_with_roles(&["viewer"])));

        let resp = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/daisy-notes/notes/note_1/share")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"permission":"viewer","sharedWithUserId":"user_2"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        let body = body_json(resp).await;
        assert_eq!(body["ok"], false);
        assert_eq!(body["error"]["code"], "DAISY_FORBIDDEN");
    }

    #[tokio::test]
    async fn daisy_share_routes_allow_owner_and_reject_public_link_payloads() {
        let router = build_daisy_contract_router(Some(principal_with_roles(&["owner"])));

        let allowed = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/daisy-notes/notes/note_1/share")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"permission":"editor","sharedWithEmail":"teammate@greenhatsec.com"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(allowed.status(), StatusCode::OK);
        let allowed_body = body_json(allowed).await;
        assert_eq!(allowed_body["ok"], true);
        assert_eq!(allowed_body["data"]["share"]["permission"], "editor");

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
}
