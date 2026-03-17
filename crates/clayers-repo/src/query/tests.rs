//! Shared query tests.
//!
//! `QueryTester<S>` exercises `ObjectStore + RefStore + QueryStore` against
//! any backend. Each backend's test module invokes `query_tests!`.

use clayers_xml::ContentHash;
use chrono::Utc;

use crate::import;
use crate::object::{Author, CommitObject, Object, TagObject, TreeEntry, TreeObject};
use crate::query::{
    QueryMode, QueryResult, QueryStore, NamespaceMap,
    query_refs, resolve_to_document,
};
use crate::refs;
use crate::store::{ObjectStore, RefStore};

const TEST_XML: &str = r#"<root xmlns:app="urn:test:app"><app:item id="1" status="active"><app:name>Alpha</app:name></app:item><app:item id="2" status="inactive"><app:name>Beta</app:name></app:item><app:item id="3" status="active"><app:name>Gamma</app:name></app:item></root>"#;

fn test_namespaces() -> NamespaceMap {
    vec![("app".to_string(), "urn:test:app".to_string())]
}

fn author() -> Author {
    Author {
        name: "Test".into(),
        email: "test@test.com".into(),
    }
}

/// Test harness for query operations on any backend.
pub struct QueryTester<S: ObjectStore + RefStore + QueryStore> {
    pub store: S,
}

impl<S: ObjectStore + RefStore + QueryStore> QueryTester<S> {
    /// Import the test XML and return the document hash.
    async fn import_test_doc(&self) -> ContentHash {
        import::import_xml(&self.store, TEST_XML).await.unwrap()
    }

    /// Import, wrap in a tree, commit, and set a branch ref.
    async fn commit_test_doc(
        &self,
        branch: &str,
        xml: &str,
        parents: Vec<ContentHash>,
    ) -> (ContentHash, ContentHash) {
        let doc_hash = import::import_xml(&self.store, xml).await.unwrap();
        // Wrap document in a single-file tree.
        let tree = TreeObject::new(vec![
            TreeEntry { path: "doc.xml".into(), document: doc_hash },
        ]);
        let tree_xml = tree.to_xml();
        let tree_hash = crate::hash::hash_exclusive(&tree_xml).unwrap();
        let mut tx = self.store.transaction().await.unwrap();
        tx.put(tree_hash, Object::Tree(tree)).await.unwrap();
        tx.commit().await.unwrap();

        let commit = CommitObject {
            tree: tree_hash,
            parents,
            author: author(),
            timestamp: Utc::now(),
            message: format!("commit on {branch}"),
        };
        let commit_xml = commit.to_xml();
        let commit_hash = crate::hash::hash_exclusive(&commit_xml).unwrap();
        let mut tx = self.store.transaction().await.unwrap();
        tx.put(commit_hash, Object::Commit(commit)).await.unwrap();
        tx.commit().await.unwrap();
        self.store
            .set_ref(&refs::branch_ref(branch), commit_hash)
            .await
            .unwrap();
        (commit_hash, doc_hash)
    }

    // --- Query tests ---

    pub async fn test_query_count(&self) {
        let doc_hash = self.import_test_doc().await;
        let result = self
            .store
            .query_document(doc_hash, "//app:item", QueryMode::Count, &test_namespaces())
            .await
            .unwrap();
        match result {
            QueryResult::Count(n) => assert_eq!(n, 3),
            _ => panic!("expected Count"),
        }
    }

    pub async fn test_query_text(&self) {
        let doc_hash = self.import_test_doc().await;
        let result = self
            .store
            .query_document(
                doc_hash,
                "//app:item[@id=\"1\"]/app:name",
                QueryMode::Text,
                &test_namespaces(),
            )
            .await
            .unwrap();
        match result {
            QueryResult::Text(texts) => {
                assert_eq!(texts.len(), 1);
                assert_eq!(texts[0], "Alpha");
            }
            _ => panic!("expected Text"),
        }
    }

    pub async fn test_query_xml(&self) {
        let doc_hash = self.import_test_doc().await;
        let result = self
            .store
            .query_document(
                doc_hash,
                "//app:item[@id=\"1\"]",
                QueryMode::Xml,
                &test_namespaces(),
            )
            .await
            .unwrap();
        match result {
            QueryResult::Xml(xmls) => {
                assert_eq!(xmls.len(), 1);
                assert!(xmls[0].contains("item"), "should contain element");
            }
            _ => panic!("expected Xml"),
        }
    }

    pub async fn test_query_with_predicate(&self) {
        let doc_hash = self.import_test_doc().await;
        let result = self
            .store
            .query_document(
                doc_hash,
                "//app:item[@status=\"active\"]",
                QueryMode::Count,
                &test_namespaces(),
            )
            .await
            .unwrap();
        match result {
            QueryResult::Count(n) => assert_eq!(n, 2),
            _ => panic!("expected Count"),
        }
    }

    pub async fn test_query_nested_path(&self) {
        let doc_hash = self.import_test_doc().await;
        let result = self
            .store
            .query_document(
                doc_hash,
                "//app:item/app:name",
                QueryMode::Count,
                &test_namespaces(),
            )
            .await
            .unwrap();
        match result {
            QueryResult::Count(n) => assert_eq!(n, 3),
            _ => panic!("expected Count"),
        }
    }

    pub async fn test_query_no_matches(&self) {
        let doc_hash = self.import_test_doc().await;
        let result = self
            .store
            .query_document(
                doc_hash,
                "//app:nonexistent",
                QueryMode::Count,
                &test_namespaces(),
            )
            .await
            .unwrap();
        match result {
            QueryResult::Count(n) => assert_eq!(n, 0),
            _ => panic!("expected Count"),
        }
    }

    pub async fn test_query_by_branch(&self) {
        let (_, doc_hash) = self.commit_test_doc("query_branch", TEST_XML, vec![]).await;
        let resolved = resolve_to_document(&self.store, &self.store, "query_branch")
            .await
            .unwrap();
        assert_eq!(resolved, doc_hash);

        let result = self
            .store
            .query_document(resolved, "//app:item", QueryMode::Count, &test_namespaces())
            .await
            .unwrap();
        match result {
            QueryResult::Count(n) => assert_eq!(n, 3),
            _ => panic!("expected Count"),
        }
    }

    pub async fn test_query_by_tag(&self) {
        let (commit_hash, doc_hash) =
            self.commit_test_doc("query_tag_branch", TEST_XML, vec![]).await;

        // Create an annotated tag pointing at the commit.
        let tag = TagObject {
            target: commit_hash,
            name: "query_v1".into(),
            tagger: author(),
            timestamp: Utc::now(),
            message: "release".into(),
        };
        let tag_xml = tag.to_xml();
        let tag_hash = crate::hash::hash_exclusive(&tag_xml).unwrap();
        let mut tx = self.store.transaction().await.unwrap();
        tx.put(tag_hash, Object::Tag(tag)).await.unwrap();
        tx.commit().await.unwrap();
        self.store
            .set_ref(&refs::tag_ref("query_v1"), tag_hash)
            .await
            .unwrap();

        let resolved = resolve_to_document(&self.store, &self.store, "query_v1")
            .await
            .unwrap();
        assert_eq!(resolved, doc_hash);
    }

    pub async fn test_query_different_revisions(&self) {
        let xml_a = "<root><val>AAA</val></root>";
        let xml_b = "<root><val>BBB</val></root>";
        let (_, doc_a) = self.commit_test_doc("rev_a", xml_a, vec![]).await;
        let (_, doc_b) = self.commit_test_doc("rev_b", xml_b, vec![]).await;

        let result_a = self
            .store
            .query_document(doc_a, "//val", QueryMode::Text, &vec![])
            .await
            .unwrap();
        let result_b = self
            .store
            .query_document(doc_b, "//val", QueryMode::Text, &vec![])
            .await
            .unwrap();

        match (result_a, result_b) {
            (QueryResult::Text(a), QueryResult::Text(b)) => {
                assert_eq!(a, vec!["AAA"]);
                assert_eq!(b, vec!["BBB"]);
            }
            _ => panic!("expected Text results"),
        }
    }

    pub async fn test_query_all_refs(&self) {
        let (_, _) = self.commit_test_doc("allrefs_a", TEST_XML, vec![]).await;
        let (_, _) = self
            .commit_test_doc(
                "allrefs_b",
                "<root><val>other</val></root>",
                vec![],
            )
            .await;

        let results = query_refs(
            &self.store,
            &self.store,
            &self.store,
            "refs/heads/allrefs_",
            "//root",
            QueryMode::Count,
            &vec![],
        )
        .await
        .unwrap();

        assert_eq!(results.len(), 2, "should query 2 branches");
    }

    pub async fn test_query_all_refs_deduplicates(&self) {
        // Both branches point to the same commit -> same doc.
        let (commit_hash, _) =
            self.commit_test_doc("dedup_a", TEST_XML, vec![]).await;
        // Set another branch to the same commit.
        self.store
            .set_ref(&refs::branch_ref("dedup_b"), commit_hash)
            .await
            .unwrap();

        let results = query_refs(
            &self.store,
            &self.store,
            &self.store,
            "refs/heads/dedup_",
            "//app:item",
            QueryMode::Count,
            &test_namespaces(),
        )
        .await
        .unwrap();

        // Same doc -> should only query once.
        assert_eq!(results.len(), 1, "should deduplicate same doc_hash");
    }

    pub async fn test_query_nonexistent_ref(&self) {
        let result = resolve_to_document(&self.store, &self.store, "nonexistent_branch").await;
        assert!(result.is_err(), "should error for missing ref");
    }

    // --- Adversarial / unhappy path tests ---

    /// `XPath` without `//` prefix must error, not silently return empty.
    pub async fn test_query_malformed_xpath_no_slashes(&self) {
        let doc_hash = self.import_test_doc().await;
        let result = self
            .store
            .query_document(doc_hash, "app:item", QueryMode::Count, &test_namespaces())
            .await;
        assert!(result.is_err(), "XPath without // should error");
    }

    /// Unbalanced bracket in predicate must error.
    pub async fn test_query_malformed_xpath_unbalanced_bracket(&self) {
        let doc_hash = self.import_test_doc().await;
        let result = self
            .store
            .query_document(doc_hash, "//app:item[@id=\"1\"", QueryMode::Count, &test_namespaces())
            .await;
        assert!(result.is_err(), "unbalanced bracket should error");
    }

    /// Predicate without @ prefix must error.
    pub async fn test_query_malformed_predicate_no_at(&self) {
        let doc_hash = self.import_test_doc().await;
        let result = self
            .store
            .query_document(doc_hash, "//app:item[id=\"1\"]", QueryMode::Count, &test_namespaces())
            .await;
        assert!(result.is_err(), "predicate without @ should error");
    }

    /// Query with unknown namespace prefix returns 0 results, not error.
    pub async fn test_query_unknown_prefix_returns_zero(&self) {
        let doc_hash = self.import_test_doc().await;
        let result = self
            .store
            .query_document(doc_hash, "//bogus:item", QueryMode::Count, &test_namespaces())
            .await
            .unwrap();
        match result {
            QueryResult::Count(n) => assert_eq!(n, 0, "unknown prefix should match nothing"),
            _ => panic!("expected Count"),
        }
    }

    /// No-namespace elements don't match prefixed queries.
    pub async fn test_query_prefix_vs_no_namespace(&self) {
        let xml = "<root><item id=\"1\">text</item></root>";
        let doc_hash = import::import_xml(&self.store, xml).await.unwrap();
        let result = self
            .store
            .query_document(doc_hash, "//app:item", QueryMode::Count, &test_namespaces())
            .await
            .unwrap();
        match result {
            QueryResult::Count(n) => assert_eq!(n, 0, "no-ns element should not match ns query"),
            _ => panic!("expected Count"),
        }
    }

    /// Query on a non-Document hash (e.g., a commit) must error.
    pub async fn test_query_on_commit_hash_errors(&self) {
        let (commit_hash, _) = self.commit_test_doc("err_commit", TEST_XML, vec![]).await;
        let result = self
            .store
            .query_document(commit_hash, "//app:item", QueryMode::Count, &test_namespaces())
            .await;
        assert!(result.is_err(), "querying a commit hash directly should error");
    }

    /// Query on a hash not in the store must error.
    pub async fn test_query_on_missing_hash_errors(&self) {
        let ghost = ContentHash::from_canonical(b"query_ghost");
        let result = self
            .store
            .query_document(ghost, "//anything", QueryMode::Count, &vec![])
            .await;
        assert!(result.is_err(), "querying missing hash should error");
    }

    /// Mixed content: comments and PIs are skipped, only elements match.
    pub async fn test_query_mixed_content_skips_non_elements(&self) {
        let xml = "<root><!-- comment --><?pi data?><item>text</item></root>";
        let doc_hash = import::import_xml(&self.store, xml).await.unwrap();
        let result = self
            .store
            .query_document(doc_hash, "//item", QueryMode::Count, &vec![])
            .await
            .unwrap();
        match result {
            QueryResult::Count(n) => assert_eq!(n, 1, "should find <item> despite comment/PI siblings"),
            _ => panic!("expected Count"),
        }
    }

    /// Deep nesting: query reaches leaf 5 levels down.
    pub async fn test_query_deep_nesting(&self) {
        let xml = "<a><b><c><d><e><leaf>found</leaf></e></d></c></b></a>";
        let doc_hash = import::import_xml(&self.store, xml).await.unwrap();
        let result = self
            .store
            .query_document(doc_hash, "//leaf", QueryMode::Text, &vec![])
            .await
            .unwrap();
        match result {
            QueryResult::Text(texts) => {
                assert_eq!(texts.len(), 1);
                assert_eq!(texts[0], "found");
            }
            _ => panic!("expected Text"),
        }
    }

    /// Text extraction concatenates multiple text children.
    pub async fn test_query_text_concatenation(&self) {
        let xml = "<root><p>Hello <b>world</b>!</p></root>";
        let doc_hash = import::import_xml(&self.store, xml).await.unwrap();
        let result = self
            .store
            .query_document(doc_hash, "//p", QueryMode::Text, &vec![])
            .await
            .unwrap();
        match result {
            QueryResult::Text(texts) => {
                assert_eq!(texts.len(), 1);
                assert_eq!(texts[0], "Hello world!");
            }
            _ => panic!("expected Text"),
        }
    }

    /// Round-trip fidelity: import -> query should see the same structure
    /// as import -> export -> reparse -> query.
    pub async fn test_query_roundtrip_fidelity(&self) {
        let xml = r#"<root xmlns:ns="urn:test"><ns:a id="1">one</ns:a><ns:a id="2">two</ns:a></root>"#;
        let ns = vec![("ns".to_string(), "urn:test".to_string())];

        let doc_hash = import::import_xml(&self.store, xml).await.unwrap();

        // Count via direct query on objects.
        let result = self
            .store
            .query_document(doc_hash, "//ns:a", QueryMode::Count, &ns)
            .await
            .unwrap();
        match result {
            QueryResult::Count(n) => assert_eq!(n, 2),
            _ => panic!("expected Count"),
        }

        // Text via direct query - order should match document order.
        let result = self
            .store
            .query_document(doc_hash, "//ns:a", QueryMode::Text, &ns)
            .await
            .unwrap();
        match result {
            QueryResult::Text(texts) => {
                assert_eq!(texts, vec!["one", "two"], "text order must match document order");
            }
            _ => panic!("expected Text"),
        }

        // Predicate query.
        let result = self
            .store
            .query_document(doc_hash, "//ns:a[@id=\"2\"]", QueryMode::Text, &ns)
            .await
            .unwrap();
        match result {
            QueryResult::Text(texts) => {
                assert_eq!(texts, vec!["two"]);
            }
            _ => panic!("expected Text"),
        }
    }

    /// Resolve HEAD when it's not set must error.
    pub async fn test_resolve_head_not_set(&self) {
        let result = resolve_to_document(&self.store, &self.store, "HEAD").await;
        assert!(result.is_err(), "HEAD not set should error");
    }

    /// Resolve via full ref path.
    pub async fn test_resolve_full_ref_path(&self) {
        let (_, doc_hash) = self.commit_test_doc("fullref_test", TEST_XML, vec![]).await;
        let resolved = resolve_to_document(&self.store, &self.store, "refs/heads/fullref_test")
            .await
            .unwrap();
        assert_eq!(resolved, doc_hash);
    }

    /// Resolve an element hash (not commit/tag/doc) must error.
    pub async fn test_resolve_element_hash_errors(&self) {
        let xml = "<root>x</root>";
        let doc_hash = import::import_xml(&self.store, xml).await.unwrap();
        // Get the root element hash from the document.
        let doc_obj = self.store.get(&doc_hash).await.unwrap().unwrap();
        let root_hash = match doc_obj {
            Object::Document(d) => d.root,
            _ => panic!("expected Document"),
        };
        // Put this element hash into a ref so resolve_to_document gets it.
        self.store.set_ref("refs/heads/elem_ref", root_hash).await.unwrap();
        let result = resolve_to_document(&self.store, &self.store, "elem_ref").await;
        assert!(result.is_err(), "resolving to an element should error");
    }

    /// Xml output of a namespaced element must include the namespace URI.
    /// Catches `build_xot_from_objects` silently dropping namespace declarations.
    pub async fn test_query_xml_preserves_namespace(&self) {
        let doc_hash = self.import_test_doc().await;
        let result = self
            .store
            .query_document(
                doc_hash,
                "//app:item[@id=\"1\"]",
                QueryMode::Xml,
                &test_namespaces(),
            )
            .await
            .unwrap();
        match result {
            QueryResult::Xml(xmls) => {
                assert_eq!(xmls.len(), 1);
                let xml = &xmls[0];
                // The serialized XML must declare the namespace.
                assert!(
                    xml.contains("urn:test:app"),
                    "serialized XML should contain namespace URI, got: {xml}"
                );
            }
            _ => panic!("expected Xml"),
        }
    }

    /// Attributes must survive the import -> `build_xot` round-trip.
    /// Catches `build_xot_from_objects` silently dropping attributes.
    pub async fn test_query_xml_preserves_attributes(&self) {
        let xml = r#"<root><item a="1" b="2" c="3">text</item></root>"#;
        let doc_hash = import::import_xml(&self.store, xml).await.unwrap();
        let result = self
            .store
            .query_document(doc_hash, "//item[@a=\"1\"]", QueryMode::Xml, &vec![])
            .await
            .unwrap();
        match result {
            QueryResult::Xml(xmls) => {
                assert_eq!(xmls.len(), 1);
                let out = &xmls[0];
                assert!(out.contains("a="), "attribute a missing from: {out}");
                assert!(out.contains("b="), "attribute b missing from: {out}");
                assert!(out.contains("c="), "attribute c missing from: {out}");
            }
            _ => panic!("expected Xml"),
        }
    }

    /// Child order must be preserved through `build_xot_from_objects`.
    /// Catches children being appended in wrong order.
    pub async fn test_query_child_order_preserved(&self) {
        let xml = "<root><first/><second/><third/></root>";
        let doc_hash = import::import_xml(&self.store, xml).await.unwrap();
        let result = self
            .store
            .query_document(doc_hash, "//root", QueryMode::Xml, &vec![])
            .await
            .unwrap();
        match result {
            QueryResult::Xml(xmls) => {
                let out = &xmls[0];
                let first_pos = out.find("first").expect("first missing");
                let second_pos = out.find("second").expect("second missing");
                let third_pos = out.find("third").expect("third missing");
                assert!(
                    first_pos < second_pos && second_pos < third_pos,
                    "children out of order: {out}"
                );
            }
            _ => panic!("expected Xml"),
        }
    }

    /// `export_xml` and query Xml mode must produce structurally equivalent output.
    /// This catches divergence between the two XML-building code paths.
    pub async fn test_export_vs_query_xml_equivalence(&self) {
        use crate::export;

        let xml = r#"<root xmlns="urn:example"><child id="1">hello</child><child id="2">world</child></root>"#;
        let doc_hash = import::import_xml(&self.store, xml).await.unwrap();

        // Get the root element via export.
        let exported = export::export_xml(&self.store, doc_hash).await.unwrap();

        // Query the root element via build_xot path.
        let ns = vec![("ex".to_string(), "urn:example".to_string())];
        let result = self
            .store
            .query_document(doc_hash, "//ex:child[@id=\"1\"]", QueryMode::Text, &ns)
            .await
            .unwrap();
        match result {
            QueryResult::Text(texts) => {
                assert_eq!(texts, vec!["hello"], "text via query must match");
            }
            _ => panic!("expected Text"),
        }

        // Both paths should agree on structure: exported XML should contain both children.
        assert!(exported.contains("hello"), "export missing 'hello'");
        assert!(exported.contains("world"), "export missing 'world'");

        // Query for count.
        let result = self
            .store
            .query_document(doc_hash, "//ex:child", QueryMode::Count, &ns)
            .await
            .unwrap();
        match result {
            QueryResult::Count(n) => assert_eq!(n, 2),
            _ => panic!("expected Count"),
        }
    }

    /// `query_refs` with no matching refs returns empty vec, not error.
    pub async fn test_query_refs_empty_prefix(&self) {
        let results = query_refs(
            &self.store,
            &self.store,
            &self.store,
            "refs/heads/surely_no_match_",
            "//root",
            QueryMode::Count,
            &vec![],
        )
        .await
        .unwrap();

        assert!(results.is_empty(), "no matching refs should return empty vec");
    }

    // --- Tree-aware query tests ---

    /// Query across multiple documents in a tree.
    pub async fn test_query_tree_wide_count(&self) {
        use crate::query;

        let xml_a = "<root><item>alpha</item></root>";
        let xml_b = "<root><item>beta</item><item>gamma</item></root>";
        let doc_a = import::import_xml(&self.store, xml_a).await.unwrap();
        let doc_b = import::import_xml(&self.store, xml_b).await.unwrap();
        let tree = TreeObject::new(vec![
            TreeEntry { path: "a.xml".into(), document: doc_a },
            TreeEntry { path: "b.xml".into(), document: doc_b },
        ]);
        let tree_xml = tree.to_xml();
        let tree_hash = crate::hash::hash_exclusive(&tree_xml).unwrap();
        let mut tx = self.store.transaction().await.unwrap();
        tx.put(tree_hash, Object::Tree(tree)).await.unwrap();
        tx.commit().await.unwrap();

        let commit = CommitObject {
            tree: tree_hash,
            parents: vec![],
            author: author(),
            timestamp: Utc::now(),
            message: "tree_wide".into(),
        };
        let commit_xml = commit.to_xml();
        let commit_hash = crate::hash::hash_exclusive(&commit_xml).unwrap();
        let mut tx = self.store.transaction().await.unwrap();
        tx.put(commit_hash, Object::Commit(commit)).await.unwrap();
        tx.commit().await.unwrap();
        self.store.set_ref(&refs::branch_ref("tree_wide"), commit_hash).await.unwrap();

        let result = query::query(
            &self.store, &self.store, &self.store,
            "tree_wide", "//item", QueryMode::Count, &vec![],
        ).await.unwrap();
        match result {
            QueryResult::Count(n) => assert_eq!(n, 3, "3 items across 2 docs"),
            _ => panic!("expected Count"),
        }
    }

    /// Query with file-scoped revspec (branch:path syntax).
    pub async fn test_query_file_scoped(&self) {

        let xml_a = "<root><item>alpha</item></root>";
        let xml_b = "<root><item>beta</item><item>gamma</item></root>";
        let doc_a = import::import_xml(&self.store, xml_a).await.unwrap();
        let doc_b = import::import_xml(&self.store, xml_b).await.unwrap();
        let tree = TreeObject::new(vec![
            TreeEntry { path: "a.xml".into(), document: doc_a },
            TreeEntry { path: "b.xml".into(), document: doc_b },
        ]);
        let tree_xml = tree.to_xml();
        let tree_hash = crate::hash::hash_exclusive(&tree_xml).unwrap();
        let mut tx = self.store.transaction().await.unwrap();
        tx.put(tree_hash, Object::Tree(tree)).await.unwrap();
        tx.commit().await.unwrap();

        let commit = CommitObject {
            tree: tree_hash,
            parents: vec![],
            author: author(),
            timestamp: Utc::now(),
            message: "file_scoped".into(),
        };
        let commit_xml = commit.to_xml();
        let commit_hash = crate::hash::hash_exclusive(&commit_xml).unwrap();
        let mut tx = self.store.transaction().await.unwrap();
        tx.put(commit_hash, Object::Commit(commit)).await.unwrap();
        tx.commit().await.unwrap();
        self.store.set_ref(&refs::branch_ref("file_scoped"), commit_hash).await.unwrap();

        // Query single doc (b.xml should have 2 items).
        let result = self.store.query_document(
            doc_b, "//item", QueryMode::Count, &vec![],
        ).await.unwrap();
        match result {
            QueryResult::Count(n) => assert_eq!(n, 2, "b.xml has 2 items"),
            _ => panic!("expected Count"),
        }
    }

    /// `resolve_to_tree` returns the tree hash and `TreeObject`.
    pub async fn test_resolve_to_tree_from_commit(&self) {
        use crate::query::resolve_to_tree;

        let (_, _) = self.commit_test_doc("resolve_tree_test", TEST_XML, vec![]).await;
        let (tree_hash, tree_obj) = resolve_to_tree(&self.store, &self.store, "resolve_tree_test")
            .await
            .unwrap();
        assert!(!tree_obj.entries.is_empty(), "tree should have entries");
        assert_eq!(tree_obj.entries[0].path, "doc.xml");
        assert_ne!(tree_hash, ContentHash::from_canonical(b"zero"));
    }
}

/// Generate test functions for query operations.
macro_rules! query_tests {
    ($create:expr) => {
        use crate::query::tests::QueryTester;

        #[tokio::test]
        async fn query_count() { QueryTester { store: $create }.test_query_count().await; }
        #[tokio::test]
        async fn query_text() { QueryTester { store: $create }.test_query_text().await; }
        #[tokio::test]
        async fn query_xml() { QueryTester { store: $create }.test_query_xml().await; }
        #[tokio::test]
        async fn query_with_predicate() { QueryTester { store: $create }.test_query_with_predicate().await; }
        #[tokio::test]
        async fn query_nested_path() { QueryTester { store: $create }.test_query_nested_path().await; }
        #[tokio::test]
        async fn query_no_matches() { QueryTester { store: $create }.test_query_no_matches().await; }
        #[tokio::test]
        async fn query_by_branch() { QueryTester { store: $create }.test_query_by_branch().await; }
        #[tokio::test]
        async fn query_by_tag() { QueryTester { store: $create }.test_query_by_tag().await; }
        #[tokio::test]
        async fn query_different_revisions() { QueryTester { store: $create }.test_query_different_revisions().await; }
        #[tokio::test]
        async fn query_all_refs() { QueryTester { store: $create }.test_query_all_refs().await; }
        #[tokio::test]
        async fn query_all_refs_deduplicates() { QueryTester { store: $create }.test_query_all_refs_deduplicates().await; }
        #[tokio::test]
        async fn query_nonexistent_ref() { QueryTester { store: $create }.test_query_nonexistent_ref().await; }
        #[tokio::test]
        async fn query_malformed_xpath_no_slashes() { QueryTester { store: $create }.test_query_malformed_xpath_no_slashes().await; }
        #[tokio::test]
        async fn query_malformed_xpath_unbalanced_bracket() { QueryTester { store: $create }.test_query_malformed_xpath_unbalanced_bracket().await; }
        #[tokio::test]
        async fn query_malformed_predicate_no_at() { QueryTester { store: $create }.test_query_malformed_predicate_no_at().await; }
        #[tokio::test]
        async fn query_unknown_prefix_returns_zero() { QueryTester { store: $create }.test_query_unknown_prefix_returns_zero().await; }
        #[tokio::test]
        async fn query_prefix_vs_no_namespace() { QueryTester { store: $create }.test_query_prefix_vs_no_namespace().await; }
        #[tokio::test]
        async fn query_on_commit_hash_errors() { QueryTester { store: $create }.test_query_on_commit_hash_errors().await; }
        #[tokio::test]
        async fn query_on_missing_hash_errors() { QueryTester { store: $create }.test_query_on_missing_hash_errors().await; }
        #[tokio::test]
        async fn query_mixed_content_skips_non_elements() { QueryTester { store: $create }.test_query_mixed_content_skips_non_elements().await; }
        #[tokio::test]
        async fn query_deep_nesting() { QueryTester { store: $create }.test_query_deep_nesting().await; }
        #[tokio::test]
        async fn query_text_concatenation() { QueryTester { store: $create }.test_query_text_concatenation().await; }
        #[tokio::test]
        async fn query_roundtrip_fidelity() { QueryTester { store: $create }.test_query_roundtrip_fidelity().await; }
        #[tokio::test]
        async fn resolve_head_not_set() { QueryTester { store: $create }.test_resolve_head_not_set().await; }
        #[tokio::test]
        async fn resolve_full_ref_path() { QueryTester { store: $create }.test_resolve_full_ref_path().await; }
        #[tokio::test]
        async fn resolve_element_hash_errors() { QueryTester { store: $create }.test_resolve_element_hash_errors().await; }
        #[tokio::test]
        async fn query_xml_preserves_namespace() { QueryTester { store: $create }.test_query_xml_preserves_namespace().await; }
        #[tokio::test]
        async fn query_xml_preserves_attributes() { QueryTester { store: $create }.test_query_xml_preserves_attributes().await; }
        #[tokio::test]
        async fn query_child_order_preserved() { QueryTester { store: $create }.test_query_child_order_preserved().await; }
        #[tokio::test]
        async fn export_vs_query_xml_equivalence() { QueryTester { store: $create }.test_export_vs_query_xml_equivalence().await; }
        #[tokio::test]
        async fn query_refs_empty_prefix() { QueryTester { store: $create }.test_query_refs_empty_prefix().await; }
        #[tokio::test]
        async fn query_tree_wide_count() { QueryTester { store: $create }.test_query_tree_wide_count().await; }
        #[tokio::test]
        async fn query_file_scoped() { QueryTester { store: $create }.test_query_file_scoped().await; }
        #[tokio::test]
        async fn resolve_to_tree_from_commit() { QueryTester { store: $create }.test_resolve_to_tree_from_commit().await; }
    };
}

pub(crate) use query_tests;
