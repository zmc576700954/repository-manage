use axum::{extract::Path, extract::State, http::StatusCode, response::Json, routing::get, Router};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::index::{Index, StoredEntry, StoredRelation};

pub const SERVER_PORT: u16 = 19181;

#[derive(Clone)]
pub struct AppState {
    pub index: Arc<RwLock<Index>>,
}

#[derive(Debug, Serialize)]
pub struct ApiError {
    pub error: String,
}

impl ApiError {
    fn new(msg: impl Into<String>) -> Self {
        Self {
            error: msg.into(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateRelationRequest {
    pub from_id: String,
    pub to_id: String,
    pub relation_type: String,
    pub note: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub data: T,
}

async fn list_entries(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<StoredEntry>>>, (StatusCode, Json<ApiError>)> {
    let index = state.index.read().await;
    let entries = index
        .get_all_entries()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError::new(e.to_string()))))?;
    Ok(Json(ApiResponse { data: entries }))
}

async fn get_entry(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StoredEntry>>, (StatusCode, Json<ApiError>)> {
    let index = state.index.read().await;
    let entries = index
        .get_all_entries()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError::new(e.to_string()))))?;
    let entry = entries
        .into_iter()
        .find(|e| e.id == id)
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(ApiError::new("Entry not found"))))?;
    Ok(Json(ApiResponse { data: entry }))
}

async fn create_relation(
    State(state): State<AppState>,
    Json(req): Json<CreateRelationRequest>,
) -> Result<Json<ApiResponse<StoredRelation>>, (StatusCode, Json<ApiError>)> {
    let index = state.index.write().await;
    index
        .add_relation(&req.from_id, &req.to_id, &req.relation_type, req.note.as_deref())
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError::new(e.to_string()))))?;

    Ok(Json(ApiResponse {
        data: StoredRelation {
            from_id: req.from_id,
            to_id: req.to_id,
            relation_type: req.relation_type,
            note: req.note,
        },
    }))
}

async fn delete_relation(
    State(state): State<AppState>,
    Path((from, to, rel_type)): Path<(String, String, String)>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let index = state.index.write().await;
    index
        .remove_relation(&from, &to, &rel_type)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError::new(e.to_string()))))?;
    Ok(StatusCode::NO_CONTENT)
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/entries", get(list_entries))
        .route("/api/entries/:id", get(get_entry))
        .route("/api/relations", axum::routing::post(create_relation))
        .route(
            "/api/relations/:from/:to/:rel_type",
            axum::routing::delete(delete_relation),
        )
        .with_state(state)
}

/// 启动 axum 服务，监听 127.0.0.1:SERVER_PORT。
pub async fn serve(state: AppState) -> anyhow::Result<()> {
    let app = router(state);
    let addr = SocketAddr::from(([127, 0, 0, 1], SERVER_PORT));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("Local server listening on {}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::RawEntry;
    use crate::index::Index;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tempfile::tempdir;
    use tower::ServiceExt;

    fn make_state() -> (AppState, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let index = Index::open(&dir.path().join("test.db")).unwrap();

        let entry = RawEntry {
            id: "topic-a".to_string(),
            title: "Topic A".to_string(),
            tags: vec!["tag1".to_string()],
            group: None,
            path: dir.path().join("topic-a"),
            content_path: dir.path().join("topic-a").join("content.md"),
            attachments: vec![],
            linked_targets: vec![],
            yaml: None,
            has_content_md: true,
        };
        index.upsert_entry(&entry).unwrap();

        let state = AppState {
            index: Arc::new(RwLock::new(index)),
        };
        (state, dir)
    }

    #[tokio::test]
    async fn list_entries_returns_ok() {
        let (state, _dir) = make_state();
        let app = router(state);

        let response = app
            .oneshot(Request::builder().uri("/api/entries").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn get_entry_returns_404_for_missing() {
        let (state, _dir) = make_state();
        let app = router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/entries/missing")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn get_entry_returns_200_for_existing() {
        let (state, _dir) = make_state();
        let app = router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/entries/topic-a")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn create_relation_returns_ok() {
        let (state, _dir) = make_state();

        // 先创建 topic-b，让外键约束满足
        let index = state.index.write().await;
        let entry_b = RawEntry {
            id: "topic-b".to_string(),
            title: "Topic B".to_string(),
            tags: vec![],
            group: None,
            path: _dir.path().join("topic-b"),
            content_path: _dir.path().join("topic-b").join("content.md"),
            attachments: vec![],
            linked_targets: vec![],
            yaml: None,
            has_content_md: true,
        };
        index.upsert_entry(&entry_b).unwrap();
        drop(index);

        let app = router(state);

        let body = serde_json::to_string(&CreateRelationRequest {
            from_id: "topic-a".to_string(),
            to_id: "topic-b".to_string(),
            relation_type: "reference".to_string(),
            note: None,
        })
        .unwrap();

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/relations")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn delete_relation_returns_204() {
        let (state, _dir) = make_state();

        // 准备两条 entry 和一条关联
        let index = state.index.write().await;
        let entry_b = RawEntry {
            id: "topic-b".to_string(),
            title: "Topic B".to_string(),
            tags: vec![],
            group: None,
            path: _dir.path().join("topic-b"),
            content_path: _dir.path().join("topic-b").join("content.md"),
            attachments: vec![],
            linked_targets: vec![],
            yaml: None,
            has_content_md: true,
        };
        index.upsert_entry(&entry_b).unwrap();
        index
            .add_relation("topic-a", "topic-b", "derived", None)
            .unwrap();
        drop(index);

        let app = router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/api/relations/topic-a/topic-b/derived")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn create_relation_with_note_persists() {
        // 准备 a 和 b 两条 entry + 一条关联（带 note）
        let (state, _dir) = make_state();

        {
            let index = state.index.write().await;
            let entry_b = RawEntry {
                id: "topic-b".to_string(),
                title: "Topic B".to_string(),
                tags: vec![],
                group: None,
                path: _dir.path().join("topic-b"),
                content_path: _dir.path().join("topic-b").join("content.md"),
                attachments: vec![],
                linked_targets: vec![],
                yaml: None,
                has_content_md: true,
            };
            index.upsert_entry(&entry_b).unwrap();
            index
                .add_relation("topic-a", "topic-b", "extends", Some("深入补充"))
                .unwrap();
        }

        let app = router(state);

        // GET /api/entries/topic-a 应该能查到带 note 的关联
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/entries/topic-a")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        // 解析 body 验证关系存在且 note 正确
        let body_bytes = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        let relations = parsed["data"]["relations"].as_array().unwrap();
        assert_eq!(relations.len(), 1);
        assert_eq!(relations[0]["relation_type"], "extends");
        assert_eq!(relations[0]["note"], "深入补充");
    }
}
