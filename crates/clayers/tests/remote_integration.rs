//! Integration test: WebSocket server + client round-trip.
//!
//! Starts a `serve_ws` server with a `SqliteStore` backend, then exercises
//! clone (`sync_refs`), push, and list-repos over the WebSocket transport.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use clayers_repo::SqliteStore;
use clayers_repo::object::{Author, TextObject, Object, DocumentObject, TreeEntry, TreeObject, CommitObject, ElementObject};
use clayers_repo::refs::HEADS_PREFIX;
use clayers_repo::store::remote::{
    BearerToken, JsonCodec, MultiTokenValidator, RemoteStore, StaticRepositories, Store,
    WsTransport, list_repositories, serve_ws,
};
use clayers_repo::store::{ObjectStore, RefStore};
use clayers_repo::sync::{FastForwardOnly, sync_refs};
use clayers_xml::ContentHash;
use tempfile::TempDir;

#[test]
#[allow(clippy::too_many_lines)]
fn websocket_clone_push_list_repos() {
    let tmp = TempDir::new().unwrap();

    // Create a SQLite store with a commit.
    let server_db = tmp.path().join("server.db");
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let store = SqliteStore::open(&server_db).unwrap();

        // Build a minimal commit: text -> element -> document -> tree -> commit
        let text = Object::Text(TextObject {
            content: "hello".into(),
        });
        let text_hash = ContentHash::from_canonical(b"text-hello");

        let elem = Object::Element(ElementObject {
            local_name: "root".into(),
            namespace_uri: None,
            namespace_prefix: None,
            extra_namespaces: vec![],
            attributes: vec![],
            children: vec![text_hash],
            inclusive_hash: ContentHash::from_canonical(b"incl-root"),
        });
        let elem_hash = ContentHash::from_canonical(b"elem-root");

        let doc = Object::Document(DocumentObject {
            root: elem_hash,
            prologue: vec![],
        });
        let doc_hash = ContentHash::from_canonical(b"doc-test");

        let tree = Object::Tree(TreeObject::new(vec![TreeEntry {
            path: "test.xml".into(),
            document: doc_hash,
        }]));
        let tree_hash = ContentHash::from_canonical(b"tree-main");

        let commit = Object::Commit(CommitObject {
            tree: tree_hash,
            parents: vec![],
            author: Author {
                name: "Test".into(),
                email: "test@test.com".into(),
            },
            timestamp: chrono::Utc::now(),
            message: "initial".into(),
        });
        let commit_hash = ContentHash::from_canonical(b"commit-initial");

        let mut tx = store.transaction().await.unwrap();
        tx.put(text_hash, text).await.unwrap();
        tx.put(elem_hash, elem).await.unwrap();
        tx.put(doc_hash, doc).await.unwrap();
        tx.put(tree_hash, tree).await.unwrap();
        tx.put(commit_hash, commit).await.unwrap();
        tx.commit().await.unwrap();

        store
            .set_ref("refs/heads/main", commit_hash)
            .await
            .unwrap();

        // Start the WS server.
        let mut repos = HashMap::new();
        repos.insert("myrepo".to_string(), Arc::new(store) as Arc<dyn Store>);
        let provider = StaticRepositories::new(repos);

        let tokens: HashSet<String> = ["test-token".to_string()].into();
        let validator = MultiTokenValidator::new(tokens);

        let (server_handle, addr) = serve_ws(
            "127.0.0.1:0",
            provider,
            Some(Box::new(validator)),
            JsonCodec,
        )
        .await
        .unwrap();

        let url = format!("ws://{addr}");

        // 1. List repos.
        let auth = BearerToken("test-token".to_string());
        let transport = WsTransport::connect(
            &url,
            JsonCodec,
            Some(&auth as &dyn clayers_repo::store::remote::WsRequestTransformer),
        )
        .await
        .unwrap();
        let repos = list_repositories(&transport).await.unwrap();
        assert!(repos.contains(&"myrepo".to_string()), "myrepo should be listed");

        // 2. Clone (sync refs from server to local).
        let local_db = tmp.path().join("local.db");
        let local = SqliteStore::open(&local_db).unwrap();

        let transport = WsTransport::connect(
            &url,
            JsonCodec,
            Some(&auth as &dyn clayers_repo::store::remote::WsRequestTransformer),
        )
        .await
        .unwrap();
        let remote = RemoteStore::new(transport, "myrepo");

        let count = sync_refs(&remote, &remote, &local, &local, HEADS_PREFIX, &FastForwardOnly)
            .await
            .unwrap();
        assert!(count > 0, "should sync at least one ref");

        // Verify the branch landed locally.
        let branches = clayers_repo::refs::list_branches(&local).await.unwrap();
        assert!(
            branches.iter().any(|(name, _)| name == "main"),
            "main branch should exist locally"
        );

        // Verify the objects transferred.
        let main_hash = local.get_ref("refs/heads/main").await.unwrap().unwrap();
        let obj = local.get(&main_hash).await.unwrap();
        assert!(obj.is_some(), "commit object should exist locally");

        // 3. Push back (should be up-to-date).
        let transport = WsTransport::connect(
            &url,
            JsonCodec,
            Some(&auth as &dyn clayers_repo::store::remote::WsRequestTransformer),
        )
        .await
        .unwrap();
        let remote2 = RemoteStore::new(transport, "myrepo");

        let push_count =
            sync_refs(&local, &local, &remote2, &remote2, HEADS_PREFIX, &FastForwardOnly)
                .await
                .unwrap();
        assert_eq!(push_count, 0, "push should be up-to-date");

        // 4. Auth rejection: connect without token should fail to list repos.
        let bad_transport = WsTransport::connect(&url, JsonCodec, None).await;
        // Connection itself may succeed but first message should fail (or connection rejected)
        if let Ok(t) = bad_transport {
            let result = list_repositories(&t).await;
            // Either the connection was rejected during handshake or the recv fails
            assert!(result.is_err(), "unauthenticated request should fail");
        }
        // If connect itself failed, that's also fine (handshake rejected)

        server_handle.abort();
    });
}
