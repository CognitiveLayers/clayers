//! Shared store trait tests.
//!
//! `StoreTester<S>` exercises `ObjectStore + RefStore` against any backend.
//! Each backend's test module invokes `store_tests!` with a constructor.

/// Generate `#[tokio::test]` functions that delegate to `StoreTester` methods.
#[cfg(test)]
macro_rules! store_tests {
    ($create:expr) => {
        use crate::store::tests::StoreTester;

        #[tokio::test]
        async fn put_and_get() { StoreTester { store: $create }.test_put_and_get().await; }
        #[tokio::test]
        async fn contains_after_commit() { StoreTester { store: $create }.test_contains_after_commit().await; }
        #[tokio::test]
        async fn rollback_discards() { StoreTester { store: $create }.test_rollback_discards().await; }
        #[tokio::test]
        async fn inclusive_hash_index() { StoreTester { store: $create }.test_inclusive_hash_index().await; }
        #[tokio::test]
        async fn cas_ref_create_if_absent() { StoreTester { store: $create }.test_cas_ref_create_if_absent().await; }
        #[tokio::test]
        async fn cas_ref_swap() { StoreTester { store: $create }.test_cas_ref_swap().await; }
        #[tokio::test]
        async fn cas_ref_reject_mismatch() { StoreTester { store: $create }.test_cas_ref_reject_mismatch().await; }
        #[tokio::test]
        async fn list_refs_with_prefix() { StoreTester { store: $create }.test_list_refs_with_prefix().await; }
        #[tokio::test]
        async fn delete_ref() { StoreTester { store: $create }.test_delete_ref().await; }
        #[tokio::test]
        async fn roundtrip_all_object_types() { StoreTester { store: $create }.test_roundtrip_all_object_types().await; }
        #[tokio::test]
        async fn subtree_document() { StoreTester { store: $create }.test_subtree_document().await; }
        #[tokio::test]
        async fn subtree_commit() { StoreTester { store: $create }.test_subtree_commit().await; }
        #[tokio::test]
        async fn subtree_diamond_dag() { StoreTester { store: $create }.test_subtree_diamond_dag().await; }
        #[tokio::test]
        async fn subtree_tag() { StoreTester { store: $create }.test_subtree_tag().await; }
        #[tokio::test]
        async fn subtree_mixed_content() { StoreTester { store: $create }.test_subtree_mixed_content().await; }
        #[tokio::test]
        async fn subtree_missing_object() { StoreTester { store: $create }.test_subtree_missing_object().await; }
        #[tokio::test]
        async fn subtree_empty_element() { StoreTester { store: $create }.test_subtree_empty_element().await; }
        #[tokio::test]
        async fn subtree_nonexistent_root() { StoreTester { store: $create }.test_subtree_nonexistent_root().await; }
        #[tokio::test]
        async fn subtree_tree() { StoreTester { store: $create }.test_subtree_tree().await; }
        #[tokio::test]
        async fn subtree_tree_shared_elements() { StoreTester { store: $create }.test_subtree_tree_shared_elements().await; }

        // ── Transaction lifecycle edges (Cat B) ──────────────────────────
        #[tokio::test]
        async fn tx_empty_commit() { StoreTester { store: $create }.test_tx_empty_commit().await; }
        #[tokio::test]
        async fn tx_drop_without_commit() { StoreTester { store: $create }.test_tx_drop_without_commit().await; }
        #[tokio::test]
        async fn tx_two_independent() { StoreTester { store: $create }.test_tx_two_independent().await; }
        #[tokio::test]
        async fn tx_many_puts() { StoreTester { store: $create }.test_tx_many_puts().await; }
        #[tokio::test]
        async fn tx_put_idempotent_within() { StoreTester { store: $create }.test_tx_put_idempotent_within().await; }
        #[tokio::test]
        async fn tx_rollback_then_new_tx() { StoreTester { store: $create }.test_tx_rollback_then_new_tx().await; }
        #[tokio::test]
        async fn tx_visibility_only_after_commit() { StoreTester { store: $create }.test_tx_visibility_only_after_commit().await; }

        // ── Ref name pathology (Cat C) ───────────────────────────────────
        #[tokio::test]
        async fn ref_unicode_name() { StoreTester { store: $create }.test_ref_unicode_name().await; }
        #[tokio::test]
        async fn ref_long_name() { StoreTester { store: $create }.test_ref_long_name().await; }
        #[tokio::test]
        async fn ref_special_chars_name() { StoreTester { store: $create }.test_ref_special_chars_name().await; }
        #[tokio::test]
        async fn ref_prefix_overlap() { StoreTester { store: $create }.test_ref_prefix_overlap().await; }
        #[tokio::test]
        async fn ref_list_empty_prefix_returns_all() { StoreTester { store: $create }.test_ref_list_empty_prefix_returns_all().await; }
        #[tokio::test]
        async fn ref_list_no_match_returns_empty() { StoreTester { store: $create }.test_ref_list_no_match_returns_empty().await; }
        #[tokio::test]
        async fn ref_set_to_unstored_hash() { StoreTester { store: $create }.test_ref_set_to_unstored_hash().await; }
        #[tokio::test]
        async fn ref_delete_nonexistent_is_noop() { StoreTester { store: $create }.test_ref_delete_nonexistent_is_noop().await; }
        #[tokio::test]
        async fn cas_with_same_expected_and_new() { StoreTester { store: $create }.test_cas_with_same_expected_and_new().await; }

        // ── Object content variants (Cat D) ──────────────────────────────
        #[tokio::test]
        async fn commit_octopus_merge() { StoreTester { store: $create }.test_commit_octopus_merge().await; }
        #[tokio::test]
        async fn element_extra_namespaces() { StoreTester { store: $create }.test_element_extra_namespaces().await; }
        #[tokio::test]
        async fn document_multi_pi_prologue() { StoreTester { store: $create }.test_document_multi_pi_prologue().await; }
        #[tokio::test]
        async fn tag_chain() { StoreTester { store: $create }.test_tag_chain().await; }
        #[tokio::test]
        async fn text_empty() { StoreTester { store: $create }.test_text_empty().await; }
        #[tokio::test]
        async fn text_large() { StoreTester { store: $create }.test_text_large().await; }
        #[tokio::test]
        async fn comment_with_newlines() { StoreTester { store: $create }.test_comment_with_newlines().await; }
        #[tokio::test]
        async fn pi_no_data() { StoreTester { store: $create }.test_pi_no_data().await; }
        #[tokio::test]
        async fn element_zero_children() { StoreTester { store: $create }.test_element_zero_children().await; }

        // ── Subtree consumer behavior (Cat E) ────────────────────────────
        #[tokio::test]
        async fn subtree_deep_chain() { StoreTester { store: $create }.test_subtree_deep_chain().await; }
        #[tokio::test]
        async fn subtree_wide_element() { StoreTester { store: $create }.test_subtree_wide_element().await; }
        #[tokio::test]
        async fn subtree_consumer_drop_safe() { StoreTester { store: $create }.test_subtree_consumer_drop_safe().await; }
        #[tokio::test]
        async fn subtree_take_one_then_continue() { StoreTester { store: $create }.test_subtree_take_one_then_continue().await; }
    };
}

#[cfg(test)]
pub(crate) use store_tests;

use std::collections::{HashMap, HashSet};

use chrono::DateTime;
use clayers_xml::ContentHash;
use tokio_stream::StreamExt;

use super::{ObjectStore, RefStore};
use crate::object::{
    Attribute, Author, CommitObject, CommentObject, DocumentObject,
    ElementObject, Object, PIObject, TagObject, TextObject, TreeEntry, TreeObject,
};

pub struct StoreTester<S: ObjectStore + RefStore> {
    pub store: S,
}

impl<S: ObjectStore + RefStore> StoreTester<S> {
    fn text_obj(s: &str) -> Object {
        Object::Text(TextObject {
            content: s.to_string(),
        })
    }

    pub async fn test_put_and_get(&self) {
        let hash = ContentHash::from_canonical(b"hello");
        let mut tx = self.store.transaction().await.unwrap();
        tx.put(hash, Self::text_obj("hello")).await.unwrap();
        tx.commit().await.unwrap();

        let obj = self.store.get(&hash).await.unwrap();
        assert!(obj.is_some());
        if let Some(Object::Text(t)) = obj {
            assert_eq!(t.content, "hello");
        } else {
            panic!("expected Text object");
        }
    }

    pub async fn test_contains_after_commit(&self) {
        let hash = ContentHash::from_canonical(b"data");
        assert!(!self.store.contains(&hash).await.unwrap());

        let mut tx = self.store.transaction().await.unwrap();
        tx.put(hash, Self::text_obj("data")).await.unwrap();
        tx.commit().await.unwrap();

        assert!(self.store.contains(&hash).await.unwrap());
    }

    pub async fn test_rollback_discards(&self) {
        let hash = ContentHash::from_canonical(b"temp");
        let mut tx = self.store.transaction().await.unwrap();
        tx.put(hash, Self::text_obj("temp")).await.unwrap();
        tx.rollback().await.unwrap();

        assert!(!self.store.contains(&hash).await.unwrap());
    }

    pub async fn test_inclusive_hash_index(&self) {
        let identity = ContentHash::from_canonical(b"exclusive");
        let inclusive = ContentHash::from_canonical(b"inclusive");

        let obj = Object::Element(ElementObject {
            local_name: "test".into(),
            namespace_uri: None,
            namespace_prefix: None,
            extra_namespaces: vec![],
            attributes: vec![],
            children: vec![],
            inclusive_hash: inclusive,
        });

        let mut tx = self.store.transaction().await.unwrap();
        tx.put(identity, obj).await.unwrap();
        tx.commit().await.unwrap();

        let result = self.store.get_by_inclusive_hash(&inclusive).await.unwrap();
        assert!(result.is_some());
        let (found_identity, _) = result.unwrap();
        assert_eq!(found_identity, identity);
    }

    pub async fn test_cas_ref_create_if_absent(&self) {
        let hash = ContentHash::from_canonical(b"v1");
        assert!(self.store.cas_ref("refs/heads/main", None, hash).await.unwrap());
        let hash2 = ContentHash::from_canonical(b"v2");
        assert!(!self.store.cas_ref("refs/heads/main", None, hash2).await.unwrap());
    }

    pub async fn test_cas_ref_swap(&self) {
        let h1 = ContentHash::from_canonical(b"v1");
        let h2 = ContentHash::from_canonical(b"v2");
        self.store.set_ref("refs/heads/cas_swap", h1).await.unwrap();
        assert!(self.store.cas_ref("refs/heads/cas_swap", Some(h1), h2).await.unwrap());
        assert_eq!(
            self.store.get_ref("refs/heads/cas_swap").await.unwrap(),
            Some(h2)
        );
    }

    pub async fn test_cas_ref_reject_mismatch(&self) {
        let h1 = ContentHash::from_canonical(b"v1");
        let h2 = ContentHash::from_canonical(b"v2");
        let h3 = ContentHash::from_canonical(b"v3");
        self.store.set_ref("refs/heads/cas_reject", h1).await.unwrap();
        assert!(!self.store.cas_ref("refs/heads/cas_reject", Some(h2), h3).await.unwrap());
        assert_eq!(
            self.store.get_ref("refs/heads/cas_reject").await.unwrap(),
            Some(h1)
        );
    }

    pub async fn test_list_refs_with_prefix(&self) {
        let h = ContentHash::from_canonical(b"list_test");
        self.store.set_ref("refs/heads/list_main", h).await.unwrap();
        self.store.set_ref("refs/heads/list_dev", h).await.unwrap();
        self.store.set_ref("refs/tags/list_v1", h).await.unwrap();

        let heads = self.store.list_refs("refs/heads/list_").await.unwrap();
        assert_eq!(heads.len(), 2);
        let tags = self.store.list_refs("refs/tags/list_").await.unwrap();
        assert_eq!(tags.len(), 1);
    }

    pub async fn test_delete_ref(&self) {
        let h = ContentHash::from_canonical(b"del_test");
        self.store.set_ref("refs/heads/del_target", h).await.unwrap();
        assert!(self.store.get_ref("refs/heads/del_target").await.unwrap().is_some());

        self.store.delete_ref("refs/heads/del_target").await.unwrap();
        assert!(self.store.get_ref("refs/heads/del_target").await.unwrap().is_none());
    }

    pub async fn test_roundtrip_all_object_types(&self) {
        let h = ContentHash::from_canonical(b"roundtrip_test");

        let objects: Vec<(ContentHash, Object)> = vec![
            (
                ContentHash::from_canonical(b"rt_text"),
                Object::Text(TextObject { content: "hello".into() }),
            ),
            (
                ContentHash::from_canonical(b"rt_comment"),
                Object::Comment(CommentObject { content: "a comment".into() }),
            ),
            (
                ContentHash::from_canonical(b"rt_pi"),
                Object::PI(PIObject {
                    target: "xml-stylesheet".into(),
                    data: Some("type=\"text/xsl\"".into()),
                }),
            ),
            (
                ContentHash::from_canonical(b"rt_element"),
                Object::Element(ElementObject {
                    local_name: "root".into(),
                    namespace_uri: Some("urn:test".into()),
                    namespace_prefix: None,
                    extra_namespaces: vec![],
                    attributes: vec![Attribute {
                        local_name: "id".into(),
                        namespace_uri: None,
                        namespace_prefix: None,
                        value: "1".into(),
                    }],
                    children: vec![h],
                    inclusive_hash: ContentHash::from_canonical(b"rt_incl"),
                }),
            ),
            (
                ContentHash::from_canonical(b"rt_doc"),
                Object::Document(DocumentObject { root: h, prologue: vec![] }),
            ),
            (
                ContentHash::from_canonical(b"rt_commit"),
                Object::Commit(CommitObject {
                    tree: h,
                    parents: vec![h],
                    author: Author { name: "Alice".into(), email: "a@b.com".into() },
                    timestamp: DateTime::parse_from_rfc3339("2026-03-17T10:30:00Z")
                        .unwrap().to_utc(),
                    message: "test".into(),
                }),
            ),
            (
                ContentHash::from_canonical(b"rt_tag"),
                Object::Tag(TagObject {
                    target: h,
                    name: "v1".into(),
                    tagger: Author { name: "Bob".into(), email: "b@c.com".into() },
                    timestamp: DateTime::parse_from_rfc3339("2026-03-17T10:30:00Z")
                        .unwrap().to_utc(),
                    message: "release".into(),
                }),
            ),
        ];

        let mut tx = self.store.transaction().await.unwrap();
        for (hash, obj) in &objects {
            tx.put(*hash, obj.clone()).await.unwrap();
        }
        tx.commit().await.unwrap();

        for (hash, expected) in &objects {
            let got = self.store.get(hash).await.unwrap()
                .expect("object should exist");
            assert_eq!(&got, expected);
        }
    }

    /// Import XML objects manually and verify subtree yields all of them.
    pub async fn test_subtree_document(&self) {
        // Build: text -> element -> document
        let text_hash = ContentHash::from_canonical(b"st_text");
        let text = TextObject { content: "hello".into() };

        let elem_hash = ContentHash::from_canonical(b"st_elem");
        let elem = ElementObject {
            local_name: "root".into(),
            namespace_uri: None,
            namespace_prefix: None,
            extra_namespaces: vec![],
            attributes: vec![],
            children: vec![text_hash],
            inclusive_hash: elem_hash,
        };

        let doc_hash = ContentHash::from_canonical(b"st_doc");
        let doc = DocumentObject { root: elem_hash, prologue: vec![] };

        let mut tx = self.store.transaction().await.unwrap();
        tx.put(text_hash, Object::Text(text)).await.unwrap();
        tx.put(elem_hash, Object::Element(elem)).await.unwrap();
        tx.put(doc_hash, Object::Document(doc)).await.unwrap();
        tx.commit().await.unwrap();

        // Collect subtree from document hash.
        let pairs: Vec<(ContentHash, Object)> = self
            .store
            .subtree(&doc_hash)
            .map(|r| r.unwrap())
            .collect()
            .await;
        let objects: HashMap<ContentHash, Object> = pairs.into_iter().collect();

        assert_eq!(objects.len(), 3, "doc + element + text = 3 objects");
        assert!(objects.contains_key(&doc_hash));
        assert!(objects.contains_key(&elem_hash));
        assert!(objects.contains_key(&text_hash));
    }

    /// Verify subtree from a commit follows tree + document + element tree.
    pub async fn test_subtree_commit(&self) {
        let text_hash = ContentHash::from_canonical(b"stc_text");
        let text = TextObject { content: "world".into() };

        let elem_hash = ContentHash::from_canonical(b"stc_elem");
        let elem = ElementObject {
            local_name: "root".into(),
            namespace_uri: None,
            namespace_prefix: None,
            extra_namespaces: vec![],
            attributes: vec![],
            children: vec![text_hash],
            inclusive_hash: elem_hash,
        };

        let doc_hash = ContentHash::from_canonical(b"stc_doc");
        let doc = DocumentObject { root: elem_hash, prologue: vec![] };

        let tree_hash = ContentHash::from_canonical(b"stc_tree");
        let tree = TreeObject::new(vec![
            TreeEntry { path: "doc.xml".into(), document: doc_hash },
        ]);

        let commit_hash = ContentHash::from_canonical(b"stc_commit");
        let commit = CommitObject {
            tree: tree_hash,
            parents: vec![],
            author: Author { name: "Test".into(), email: "t@t.com".into() },
            timestamp: DateTime::parse_from_rfc3339("2026-03-17T10:30:00Z")
                .unwrap().to_utc(),
            message: "test".into(),
        };

        let mut tx = self.store.transaction().await.unwrap();
        tx.put(text_hash, Object::Text(text)).await.unwrap();
        tx.put(elem_hash, Object::Element(elem)).await.unwrap();
        tx.put(doc_hash, Object::Document(doc)).await.unwrap();
        tx.put(tree_hash, Object::Tree(tree)).await.unwrap();
        tx.put(commit_hash, Object::Commit(commit)).await.unwrap();
        tx.commit().await.unwrap();

        let pairs: Vec<(ContentHash, Object)> = self
            .store
            .subtree(&commit_hash)
            .map(|r| r.unwrap())
            .collect()
            .await;
        let objects: HashMap<ContentHash, Object> = pairs.into_iter().collect();

        assert_eq!(objects.len(), 5, "commit + tree + doc + element + text = 5");
        assert!(objects.contains_key(&commit_hash));
        assert!(objects.contains_key(&tree_hash));
        assert!(objects.contains_key(&doc_hash));
        assert!(objects.contains_key(&elem_hash));
        assert!(objects.contains_key(&text_hash));
    }

    /// Two elements share the same child text node (diamond DAG).
    /// subtree must yield the shared node exactly once.
    pub async fn test_subtree_diamond_dag(&self) {
        let shared_text_hash = ContentHash::from_canonical(b"diamond_text");
        let shared_text = TextObject { content: "shared".into() };

        let left_hash = ContentHash::from_canonical(b"diamond_a");
        let left_elem = ElementObject {
            local_name: "a".into(),
            namespace_uri: None,
            namespace_prefix: None,
            extra_namespaces: vec![],
            attributes: vec![],
            children: vec![shared_text_hash],
            inclusive_hash: left_hash,
        };

        let right_hash = ContentHash::from_canonical(b"diamond_b");
        let right_elem = ElementObject {
            local_name: "b".into(),
            namespace_uri: None,
            namespace_prefix: None,
            extra_namespaces: vec![],
            attributes: vec![],
            children: vec![shared_text_hash],
            inclusive_hash: right_hash,
        };

        let root_hash = ContentHash::from_canonical(b"diamond_root");
        let root = ElementObject {
            local_name: "root".into(),
            namespace_uri: None,
            namespace_prefix: None,
            extra_namespaces: vec![],
            attributes: vec![],
            children: vec![left_hash, right_hash],
            inclusive_hash: root_hash,
        };

        let doc_hash = ContentHash::from_canonical(b"diamond_doc");
        let doc = DocumentObject { root: root_hash, prologue: vec![] };

        let mut tx = self.store.transaction().await.unwrap();
        tx.put(shared_text_hash, Object::Text(shared_text)).await.unwrap();
        tx.put(left_hash, Object::Element(left_elem)).await.unwrap();
        tx.put(right_hash, Object::Element(right_elem)).await.unwrap();
        tx.put(root_hash, Object::Element(root)).await.unwrap();
        tx.put(doc_hash, Object::Document(doc)).await.unwrap();
        tx.commit().await.unwrap();

        let pairs: Vec<(ContentHash, Object)> = self
            .store
            .subtree(&doc_hash)
            .map(|r| r.unwrap())
            .collect()
            .await;
        let objects: HashMap<ContentHash, Object> = pairs.into_iter().collect();

        // doc + root + a + b + shared_text = 5, NOT 6
        assert_eq!(objects.len(), 5, "shared text node must appear once, not twice");
        assert!(objects.contains_key(&shared_text_hash));
    }

    /// subtree from a tag follows target through commit to tree to document.
    pub async fn test_subtree_tag(&self) {
        let text_hash = ContentHash::from_canonical(b"tag_st_text");
        let elem_hash = ContentHash::from_canonical(b"tag_st_elem");
        let doc_hash = ContentHash::from_canonical(b"tag_st_doc");
        let tree_hash = ContentHash::from_canonical(b"tag_st_tree");
        let commit_hash = ContentHash::from_canonical(b"tag_st_commit");
        let tag_hash = ContentHash::from_canonical(b"tag_st_tag");

        let mut tx = self.store.transaction().await.unwrap();
        tx.put(text_hash, Object::Text(TextObject { content: "t".into() })).await.unwrap();
        tx.put(elem_hash, Object::Element(ElementObject {
            local_name: "r".into(),
            namespace_uri: None,
            namespace_prefix: None,
            extra_namespaces: vec![],
            attributes: vec![],
            children: vec![text_hash],
            inclusive_hash: elem_hash,
        })).await.unwrap();
        tx.put(doc_hash, Object::Document(DocumentObject { root: elem_hash, prologue: vec![] })).await.unwrap();
        tx.put(tree_hash, Object::Tree(TreeObject::new(vec![
            TreeEntry { path: "doc.xml".into(), document: doc_hash },
        ]))).await.unwrap();
        tx.put(commit_hash, Object::Commit(CommitObject {
            tree: tree_hash,
            parents: vec![],
            author: Author { name: "T".into(), email: "t@t".into() },
            timestamp: DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z").unwrap().to_utc(),
            message: "m".into(),
        })).await.unwrap();
        tx.put(tag_hash, Object::Tag(TagObject {
            target: commit_hash,
            name: "v1".into(),
            tagger: Author { name: "T".into(), email: "t@t".into() },
            timestamp: DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z").unwrap().to_utc(),
            message: "release".into(),
        })).await.unwrap();
        tx.commit().await.unwrap();

        let pairs: Vec<(ContentHash, Object)> = self
            .store
            .subtree(&tag_hash)
            .map(|r| r.unwrap())
            .collect()
            .await;
        let objects: HashMap<ContentHash, Object> = pairs.into_iter().collect();

        // tag + commit + tree + doc + elem + text = 6
        assert_eq!(objects.len(), 6);
        assert!(objects.contains_key(&tag_hash));
        assert!(objects.contains_key(&tree_hash));
        assert!(objects.contains_key(&text_hash));
    }

    /// subtree over element with comment and PI children.
    pub async fn test_subtree_mixed_content(&self) {
        let text_hash = ContentHash::from_canonical(b"mix_text");
        let comment_hash = ContentHash::from_canonical(b"mix_comment");
        let pi_hash = ContentHash::from_canonical(b"mix_pi");
        let elem_hash = ContentHash::from_canonical(b"mix_elem");
        let doc_hash = ContentHash::from_canonical(b"mix_doc");

        let mut tx = self.store.transaction().await.unwrap();
        tx.put(text_hash, Object::Text(TextObject { content: "hello".into() })).await.unwrap();
        tx.put(comment_hash, Object::Comment(CommentObject { content: "a comment".into() })).await.unwrap();
        tx.put(pi_hash, Object::PI(PIObject { target: "app".into(), data: Some("v=1".into()) })).await.unwrap();
        tx.put(elem_hash, Object::Element(ElementObject {
            local_name: "root".into(),
            namespace_uri: None,
            namespace_prefix: None,
            extra_namespaces: vec![],
            attributes: vec![],
            children: vec![text_hash, comment_hash, pi_hash],
            inclusive_hash: elem_hash,
        })).await.unwrap();
        tx.put(doc_hash, Object::Document(DocumentObject { root: elem_hash, prologue: vec![] })).await.unwrap();
        tx.commit().await.unwrap();

        let pairs: Vec<(ContentHash, Object)> = self
            .store
            .subtree(&doc_hash)
            .map(|r| r.unwrap())
            .collect()
            .await;
        let objects: HashMap<ContentHash, Object> = pairs.into_iter().collect();

        assert_eq!(objects.len(), 5, "doc + elem + text + comment + pi");
        assert!(objects.contains_key(&comment_hash));
        assert!(objects.contains_key(&pi_hash));
    }

    /// subtree errors on a missing object mid-walk.
    pub async fn test_subtree_missing_object(&self) {
        let missing_child = ContentHash::from_canonical(b"subtree_ghost");
        let elem_hash = ContentHash::from_canonical(b"subtree_parent");
        let doc_hash = ContentHash::from_canonical(b"subtree_doc_missing");

        let mut tx = self.store.transaction().await.unwrap();
        // Element references missing_child, which is NOT stored.
        tx.put(elem_hash, Object::Element(ElementObject {
            local_name: "root".into(),
            namespace_uri: None,
            namespace_prefix: None,
            extra_namespaces: vec![],
            attributes: vec![],
            children: vec![missing_child],
            inclusive_hash: elem_hash,
        })).await.unwrap();
        tx.put(doc_hash, Object::Document(DocumentObject { root: elem_hash, prologue: vec![] })).await.unwrap();
        tx.commit().await.unwrap();

        let results: Vec<crate::error::Result<(ContentHash, Object)>> = self
            .store
            .subtree(&doc_hash)
            .collect()
            .await;

        let has_error = results.iter().any(std::result::Result::is_err);
        assert!(has_error, "subtree should error when a referenced object is missing");
    }

    /// subtree on a leaf element with no children.
    pub async fn test_subtree_empty_element(&self) {
        let elem_hash = ContentHash::from_canonical(b"empty_elem");
        let doc_hash = ContentHash::from_canonical(b"empty_doc");

        let mut tx = self.store.transaction().await.unwrap();
        tx.put(elem_hash, Object::Element(ElementObject {
            local_name: "empty".into(),
            namespace_uri: None,
            namespace_prefix: None,
            extra_namespaces: vec![],
            attributes: vec![Attribute {
                local_name: "id".into(),
                namespace_uri: None,
                namespace_prefix: None,
                value: "1".into(),
            }],
            children: vec![],
            inclusive_hash: elem_hash,
        })).await.unwrap();
        tx.put(doc_hash, Object::Document(DocumentObject { root: elem_hash, prologue: vec![] })).await.unwrap();
        tx.commit().await.unwrap();

        let pairs: Vec<(ContentHash, Object)> = self
            .store
            .subtree(&doc_hash)
            .map(|r| r.unwrap())
            .collect()
            .await;
        let objects: HashMap<ContentHash, Object> = pairs.into_iter().collect();

        assert_eq!(objects.len(), 2, "doc + empty element");
    }

    /// subtree from a hash that doesn't exist at all.
    pub async fn test_subtree_nonexistent_root(&self) {
        let ghost = ContentHash::from_canonical(b"total_ghost");
        let results: Vec<crate::error::Result<(ContentHash, Object)>> = self
            .store
            .subtree(&ghost)
            .collect()
            .await;

        assert_eq!(results.len(), 1);
        assert!(results[0].is_err(), "subtree from nonexistent root should error");
    }

    /// subtree from a tree follows all document entries to elements.
    pub async fn test_subtree_tree(&self) {
        let text1 = ContentHash::from_canonical(b"st_tree_text1");
        let elem1 = ContentHash::from_canonical(b"st_tree_elem1");
        let doc1 = ContentHash::from_canonical(b"st_tree_doc1");

        let text2 = ContentHash::from_canonical(b"st_tree_text2");
        let elem2 = ContentHash::from_canonical(b"st_tree_elem2");
        let doc2 = ContentHash::from_canonical(b"st_tree_doc2");

        let tree_hash = ContentHash::from_canonical(b"st_tree_tree");

        let mut tx = self.store.transaction().await.unwrap();
        tx.put(text1, Object::Text(TextObject { content: "one".into() })).await.unwrap();
        tx.put(elem1, Object::Element(ElementObject {
            local_name: "r".into(),
            namespace_uri: None,
            namespace_prefix: None,
            extra_namespaces: vec![],
            attributes: vec![],
            children: vec![text1],
            inclusive_hash: elem1,
        })).await.unwrap();
        tx.put(doc1, Object::Document(DocumentObject { root: elem1, prologue: vec![] })).await.unwrap();

        tx.put(text2, Object::Text(TextObject { content: "two".into() })).await.unwrap();
        tx.put(elem2, Object::Element(ElementObject {
            local_name: "r".into(),
            namespace_uri: None,
            namespace_prefix: None,
            extra_namespaces: vec![],
            attributes: vec![],
            children: vec![text2],
            inclusive_hash: elem2,
        })).await.unwrap();
        tx.put(doc2, Object::Document(DocumentObject { root: elem2, prologue: vec![] })).await.unwrap();

        tx.put(tree_hash, Object::Tree(TreeObject::new(vec![
            TreeEntry { path: "a.xml".into(), document: doc1 },
            TreeEntry { path: "b.xml".into(), document: doc2 },
        ]))).await.unwrap();
        tx.commit().await.unwrap();

        let pairs: Vec<(ContentHash, Object)> = self
            .store
            .subtree(&tree_hash)
            .map(|r| r.unwrap())
            .collect()
            .await;
        let objects: HashMap<ContentHash, Object> = pairs.into_iter().collect();

        // tree + 2*(doc + elem + text) = 7
        assert_eq!(objects.len(), 7);
        assert!(objects.contains_key(&tree_hash));
        assert!(objects.contains_key(&doc1));
        assert!(objects.contains_key(&doc2));
    }

    /// Tree with two documents sharing the same element, yielded once.
    pub async fn test_subtree_tree_shared_elements(&self) {
        let shared_text = ContentHash::from_canonical(b"st_tree_shared_text");
        let shared_elem = ContentHash::from_canonical(b"st_tree_shared_elem");
        let doc1 = ContentHash::from_canonical(b"st_tree_shared_doc1");
        let doc2 = ContentHash::from_canonical(b"st_tree_shared_doc2");
        let tree_hash = ContentHash::from_canonical(b"st_tree_shared_tree");

        let mut tx = self.store.transaction().await.unwrap();
        tx.put(shared_text, Object::Text(TextObject { content: "shared".into() })).await.unwrap();
        tx.put(shared_elem, Object::Element(ElementObject {
            local_name: "r".into(),
            namespace_uri: None,
            namespace_prefix: None,
            extra_namespaces: vec![],
            attributes: vec![],
            children: vec![shared_text],
            inclusive_hash: shared_elem,
        })).await.unwrap();
        // Both docs point to same root element.
        tx.put(doc1, Object::Document(DocumentObject { root: shared_elem, prologue: vec![] })).await.unwrap();
        tx.put(doc2, Object::Document(DocumentObject { root: shared_elem, prologue: vec![] })).await.unwrap();
        tx.put(tree_hash, Object::Tree(TreeObject::new(vec![
            TreeEntry { path: "a.xml".into(), document: doc1 },
            TreeEntry { path: "b.xml".into(), document: doc2 },
        ]))).await.unwrap();
        tx.commit().await.unwrap();

        let pairs: Vec<(ContentHash, Object)> = self
            .store
            .subtree(&tree_hash)
            .map(|r| r.unwrap())
            .collect()
            .await;
        let objects: HashMap<ContentHash, Object> = pairs.into_iter().collect();

        // tree + doc1 + doc2 + shared_elem + shared_text = 5 (NOT 7)
        assert_eq!(objects.len(), 5, "shared element must appear once");
    }

    // ── Transaction lifecycle edges (Cat B) ─────────────────────────────

    /// Empty commit: opening a transaction and committing with no puts
    /// must succeed without erroring, and must not disturb data already
    /// committed in earlier transactions.
    pub async fn test_tx_empty_commit(&self) {
        // Commit a real object first.
        let h = ContentHash::from_canonical(b"tx_empty_existing");
        let mut tx0 = self.store.transaction().await.unwrap();
        tx0.put(h, Self::text_obj("existing")).await.unwrap();
        tx0.commit().await.unwrap();

        // Open and commit an empty transaction.
        let mut tx = self.store.transaction().await.unwrap();
        tx.commit().await.unwrap();

        // The earlier object must still be present and intact.
        assert!(self.store.contains(&h).await.unwrap());
        assert_eq!(
            self.store.get(&h).await.unwrap(),
            Some(Self::text_obj("existing")),
        );
    }

    /// A transaction dropped without commit or rollback must not persist
    /// any of its pending writes.
    pub async fn test_tx_drop_without_commit(&self) {
        let h = ContentHash::from_canonical(b"tx_drop_target");
        {
            let mut tx = self.store.transaction().await.unwrap();
            tx.put(h, Self::text_obj("ghost")).await.unwrap();
            // tx dropped here, no commit, no rollback
        }
        assert!(!self.store.contains(&h).await.unwrap());
    }

    /// Two transactions opened in sequence must be independent: rollback
    /// of one must not affect commits of the other.
    pub async fn test_tx_two_independent(&self) {
        let h1 = ContentHash::from_canonical(b"tx_indep_keep");
        let h2 = ContentHash::from_canonical(b"tx_indep_drop");

        let mut tx1 = self.store.transaction().await.unwrap();
        tx1.put(h1, Self::text_obj("keep")).await.unwrap();
        tx1.commit().await.unwrap();

        let mut tx2 = self.store.transaction().await.unwrap();
        tx2.put(h2, Self::text_obj("drop")).await.unwrap();
        tx2.rollback().await.unwrap();

        assert!(self.store.contains(&h1).await.unwrap());
        assert!(!self.store.contains(&h2).await.unwrap());
    }

    /// Many puts in a single transaction must all become visible after
    /// commit. Stresses batching and any single-statement caps.
    pub async fn test_tx_many_puts(&self) {
        const N: usize = 1000;

        let mut tx = self.store.transaction().await.unwrap();
        let mut hashes = Vec::with_capacity(N);
        for i in 0..N {
            let h = ContentHash::from_canonical(format!("tx_many_{i}").as_bytes());
            tx.put(h, Self::text_obj(&format!("v{i}"))).await.unwrap();
            hashes.push(h);
        }
        tx.commit().await.unwrap();

        for h in &hashes {
            assert!(self.store.contains(h).await.unwrap());
        }
    }

    /// Putting the same (hash, object) twice in one transaction must
    /// not error, and the object must be readable after commit.
    /// Note: content-addressing means duplicates of the same hash are
    /// inherently coalesced; this test specifically verifies the
    /// double-put path does not raise an error or corrupt state.
    pub async fn test_tx_put_idempotent_within(&self) {
        let h = ContentHash::from_canonical(b"tx_idem_within");
        let obj = Self::text_obj("same");

        let mut tx = self.store.transaction().await.unwrap();
        tx.put(h, obj.clone()).await.unwrap();
        tx.put(h, obj.clone()).await.unwrap();
        tx.commit().await.unwrap();

        let got = self.store.get(&h).await.unwrap();
        assert_eq!(got, Some(obj));
    }

    /// After rollback, opening a fresh transaction and committing must
    /// work normally.
    pub async fn test_tx_rollback_then_new_tx(&self) {
        let h_drop = ContentHash::from_canonical(b"tx_after_rb_drop");
        let h_keep = ContentHash::from_canonical(b"tx_after_rb_keep");

        let mut tx1 = self.store.transaction().await.unwrap();
        tx1.put(h_drop, Self::text_obj("drop")).await.unwrap();
        tx1.rollback().await.unwrap();

        let mut tx2 = self.store.transaction().await.unwrap();
        tx2.put(h_keep, Self::text_obj("keep")).await.unwrap();
        tx2.commit().await.unwrap();

        assert!(!self.store.contains(&h_drop).await.unwrap());
        assert!(self.store.contains(&h_keep).await.unwrap());
    }

    /// Inside an open transaction, puts must be invisible to readers
    /// going through the store directly (no dirty reads).
    pub async fn test_tx_visibility_only_after_commit(&self) {
        let h = ContentHash::from_canonical(b"tx_vis_target");

        let mut tx = self.store.transaction().await.unwrap();
        tx.put(h, Self::text_obj("staged")).await.unwrap();

        // Before commit: not visible.
        assert!(!self.store.contains(&h).await.unwrap());
        assert!(self.store.get(&h).await.unwrap().is_none());

        tx.commit().await.unwrap();

        // After commit: visible.
        assert!(self.store.contains(&h).await.unwrap());
    }

    // ── Ref name pathology (Cat C) ─────────────────────────────────────

    /// Ref names containing non-ASCII Unicode must roundtrip.
    pub async fn test_ref_unicode_name(&self) {
        let name = "refs/heads/café-日本語-🚀";
        let h = ContentHash::from_canonical(b"unicode_target");
        self.store.set_ref(name, h).await.unwrap();
        assert_eq!(self.store.get_ref(name).await.unwrap(), Some(h));
    }

    /// Very long ref names (1000+ chars) must roundtrip.
    pub async fn test_ref_long_name(&self) {
        let suffix: String = "x".repeat(1000);
        let name = format!("refs/heads/{suffix}");
        let h = ContentHash::from_canonical(b"long_target");
        self.store.set_ref(&name, h).await.unwrap();
        assert_eq!(self.store.get_ref(&name).await.unwrap(), Some(h));
    }

    /// Ref names with special characters (periods, colons, plus, equals)
    /// must roundtrip.
    pub async fn test_ref_special_chars_name(&self) {
        let names = [
            "refs/heads/feat.with.dots",
            "refs/heads/feat+with+plus",
            "refs/heads/feat=with=eq",
            "refs/tags/v1.2.3-rc.1+build.42",
            "refs/heads/with spaces",
        ];
        let h = ContentHash::from_canonical(b"special_target");
        for name in &names {
            self.store.set_ref(name, h).await.unwrap();
            assert_eq!(self.store.get_ref(name).await.unwrap(), Some(h));
        }
    }

    /// Refs with overlapping names must not be confused: `list_refs` with
    /// a precise prefix returns only the precisely matching subset.
    pub async fn test_ref_prefix_overlap(&self) {
        let h = ContentHash::from_canonical(b"prefix_overlap_target");
        self.store.set_ref("refs/heads/main", h).await.unwrap();
        self.store.set_ref("refs/heads/main-2", h).await.unwrap();
        self.store.set_ref("refs/heads/main/sub", h).await.unwrap();
        self.store.set_ref("refs/heads/feature", h).await.unwrap();

        // "refs/heads/main" matches all three "main*" refs (starts_with).
        let matches = self.store.list_refs("refs/heads/main").await.unwrap();
        let names: HashSet<&str> = matches.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names.len(), 3);
        assert!(names.contains("refs/heads/main"));
        assert!(names.contains("refs/heads/main-2"));
        assert!(names.contains("refs/heads/main/sub"));
        assert!(!names.contains("refs/heads/feature"));

        // "refs/heads/main/" matches only the slash-suffixed child.
        let children = self.store.list_refs("refs/heads/main/").await.unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].0, "refs/heads/main/sub");
    }

    /// `list_refs` with empty prefix returns all refs in the store.
    pub async fn test_ref_list_empty_prefix_returns_all(&self) {
        let h = ContentHash::from_canonical(b"empty_prefix_target");
        self.store.set_ref("refs/heads/a", h).await.unwrap();
        self.store.set_ref("refs/tags/v1", h).await.unwrap();
        self.store.set_ref("HEAD", h).await.unwrap();

        let all = self.store.list_refs("").await.unwrap();
        assert!(all.len() >= 3);
        let names: HashSet<&str> = all.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains("refs/heads/a"));
        assert!(names.contains("refs/tags/v1"));
        assert!(names.contains("HEAD"));
    }

    /// `list_refs` with a prefix that matches nothing returns an empty list,
    /// not an error.
    pub async fn test_ref_list_no_match_returns_empty(&self) {
        let h = ContentHash::from_canonical(b"no_match_target");
        self.store.set_ref("refs/heads/main", h).await.unwrap();

        let none = self.store.list_refs("refs/no/such/prefix/").await.unwrap();
        assert!(none.is_empty());
    }

    /// `set_ref` to a hash whose object is not stored is allowed (refs are
    /// pointers; integrity is the caller's responsibility). The test
    /// explicitly verifies the "unstored" premise: the object is *not*
    /// in the store, yet the ref can still point at it.
    pub async fn test_ref_set_to_unstored_hash(&self) {
        let dangling = ContentHash::from_canonical(b"never_stored");

        // Premise: the object is not in the store.
        assert!(!self.store.contains(&dangling).await.unwrap());
        assert!(self.store.get(&dangling).await.unwrap().is_none());

        // The ref can still be set to point at it.
        self.store.set_ref("refs/heads/dangling", dangling).await.unwrap();
        assert_eq!(
            self.store.get_ref("refs/heads/dangling").await.unwrap(),
            Some(dangling),
        );

        // After set_ref, the object remains unstored (set_ref does not
        // imply object insertion).
        assert!(!self.store.contains(&dangling).await.unwrap());
    }

    /// Deleting a ref that does not exist is a no-op (no error).
    pub async fn test_ref_delete_nonexistent_is_noop(&self) {
        // Should not panic or error.
        self.store.delete_ref("refs/heads/never_existed").await.unwrap();
        assert!(self.store.get_ref("refs/heads/never_existed").await.unwrap().is_none());
    }

    /// CAS where expected == new is a successful no-op when the ref already
    /// holds that value, and fails when it doesn't.
    pub async fn test_cas_with_same_expected_and_new(&self) {
        let h = ContentHash::from_canonical(b"cas_same_target");
        self.store.set_ref("refs/heads/cas_same", h).await.unwrap();

        // expected = current = new : succeeds
        assert!(self.store.cas_ref("refs/heads/cas_same", Some(h), h).await.unwrap());

        // expected != current : fails
        let other = ContentHash::from_canonical(b"cas_same_other");
        assert!(!self.store.cas_ref("refs/heads/cas_same", Some(other), h).await.unwrap());
    }

    // ── Object content variants (Cat D) ───────────────────────────────

    /// A commit with many parents (octopus merge) must roundtrip exactly.
    pub async fn test_commit_octopus_merge(&self) {
        let tree_hash = ContentHash::from_canonical(b"octo_tree");
        let parents: Vec<ContentHash> = (0..8)
            .map(|i| ContentHash::from_canonical(format!("octo_parent_{i}").as_bytes()))
            .collect();

        let commit = CommitObject {
            tree: tree_hash,
            parents: parents.clone(),
            author: Author { name: "Octo".into(), email: "o@o.com".into() },
            timestamp: DateTime::parse_from_rfc3339("2026-03-17T10:30:00Z")
                .unwrap().to_utc(),
            message: "octopus".into(),
        };
        let commit_hash = ContentHash::from_canonical(b"octo_commit");

        let mut tx = self.store.transaction().await.unwrap();
        tx.put(commit_hash, Object::Commit(commit.clone())).await.unwrap();
        tx.commit().await.unwrap();

        let got = self.store.get(&commit_hash).await.unwrap().unwrap();
        if let Object::Commit(c) = got {
            assert_eq!(c.parents, parents);
            assert_eq!(c.parents.len(), 8);
        } else {
            panic!("expected Commit");
        }
    }

    /// Element with extra namespace declarations and prefixed attributes
    /// must roundtrip with all namespace metadata intact.
    pub async fn test_element_extra_namespaces(&self) {
        let elem_hash = ContentHash::from_canonical(b"ns_elem");
        let elem = ElementObject {
            local_name: "root".into(),
            namespace_uri: Some("urn:default".into()),
            namespace_prefix: None,
            extra_namespaces: vec![
                ("a".into(), "urn:nsA".into()),
                ("b".into(), "urn:nsB".into()),
            ],
            attributes: vec![
                Attribute {
                    local_name: "id".into(),
                    namespace_uri: Some("urn:nsA".into()),
                    namespace_prefix: Some("a".into()),
                    value: "1".into(),
                },
            ],
            children: vec![],
            inclusive_hash: ContentHash::from_canonical(b"ns_incl"),
        };

        let mut tx = self.store.transaction().await.unwrap();
        tx.put(elem_hash, Object::Element(elem.clone())).await.unwrap();
        tx.commit().await.unwrap();

        let got = self.store.get(&elem_hash).await.unwrap().unwrap();
        assert_eq!(got, Object::Element(elem));
    }

    /// A document with multiple processing instructions in the prologue
    /// must roundtrip with prologue order preserved.
    pub async fn test_document_multi_pi_prologue(&self) {
        let pi1_hash = ContentHash::from_canonical(b"pi1");
        let pi2_hash = ContentHash::from_canonical(b"pi2");
        let comment_hash = ContentHash::from_canonical(b"prologue_comment");
        let elem_hash = ContentHash::from_canonical(b"prologue_root");

        let pi1 = PIObject { target: "xml-stylesheet".into(), data: Some("href=\"a.xsl\"".into()) };
        let pi2 = PIObject { target: "app".into(), data: Some("v=1".into()) };
        let comment = CommentObject { content: " a header comment ".into() };
        let elem = ElementObject {
            local_name: "root".into(),
            namespace_uri: None,
            namespace_prefix: None,
            extra_namespaces: vec![],
            attributes: vec![],
            children: vec![],
            inclusive_hash: elem_hash,
        };

        let prologue = vec![pi1_hash, comment_hash, pi2_hash];
        let doc_hash = ContentHash::from_canonical(b"prologue_doc");
        let doc = DocumentObject { root: elem_hash, prologue: prologue.clone() };

        let mut tx = self.store.transaction().await.unwrap();
        tx.put(pi1_hash, Object::PI(pi1)).await.unwrap();
        tx.put(pi2_hash, Object::PI(pi2)).await.unwrap();
        tx.put(comment_hash, Object::Comment(comment)).await.unwrap();
        tx.put(elem_hash, Object::Element(elem)).await.unwrap();
        tx.put(doc_hash, Object::Document(doc)).await.unwrap();
        tx.commit().await.unwrap();

        let got = self.store.get(&doc_hash).await.unwrap().unwrap();
        if let Object::Document(d) = got {
            assert_eq!(d.prologue, prologue, "prologue order must be preserved");
        } else {
            panic!("expected Document");
        }
    }

    /// A tag pointing at another tag must roundtrip and subtree must
    /// follow the chain.
    pub async fn test_tag_chain(&self) {
        let inner_target = ContentHash::from_canonical(b"chain_target");
        let inner_target_obj = TextObject { content: "leaf".into() };

        let inner_tag_hash = ContentHash::from_canonical(b"chain_inner_tag");
        let inner_tag = TagObject {
            target: inner_target,
            name: "inner".into(),
            tagger: Author { name: "T".into(), email: "t@t".into() },
            timestamp: DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z").unwrap().to_utc(),
            message: "inner".into(),
        };

        let outer_tag_hash = ContentHash::from_canonical(b"chain_outer_tag");
        let outer_tag = TagObject {
            target: inner_tag_hash,
            name: "outer".into(),
            tagger: Author { name: "T".into(), email: "t@t".into() },
            timestamp: DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z").unwrap().to_utc(),
            message: "outer".into(),
        };

        let mut tx = self.store.transaction().await.unwrap();
        tx.put(inner_target, Object::Text(inner_target_obj)).await.unwrap();
        tx.put(inner_tag_hash, Object::Tag(inner_tag)).await.unwrap();
        tx.put(outer_tag_hash, Object::Tag(outer_tag)).await.unwrap();
        tx.commit().await.unwrap();

        let pairs: Vec<(ContentHash, Object)> = self
            .store
            .subtree(&outer_tag_hash)
            .map(|r| r.unwrap())
            .collect()
            .await;
        let objects: HashMap<ContentHash, Object> = pairs.into_iter().collect();

        // outer_tag + inner_tag + inner_target = 3
        assert_eq!(objects.len(), 3);
        assert!(objects.contains_key(&outer_tag_hash));
        assert!(objects.contains_key(&inner_tag_hash));
        assert!(objects.contains_key(&inner_target));
    }

    /// An empty text object must roundtrip.
    pub async fn test_text_empty(&self) {
        let h = ContentHash::from_canonical(b"empty_text");
        let mut tx = self.store.transaction().await.unwrap();
        tx.put(h, Object::Text(TextObject { content: String::new() })).await.unwrap();
        tx.commit().await.unwrap();

        let got = self.store.get(&h).await.unwrap().unwrap();
        if let Object::Text(t) = got {
            assert_eq!(t.content, "");
        } else {
            panic!("expected Text");
        }
    }

    /// A large text object (~1 MB) must roundtrip without truncation.
    pub async fn test_text_large(&self) {
        let content = "x".repeat(1_000_000);
        let h = ContentHash::from_canonical(b"large_text");
        let mut tx = self.store.transaction().await.unwrap();
        tx.put(h, Object::Text(TextObject { content: content.clone() })).await.unwrap();
        tx.commit().await.unwrap();

        let got = self.store.get(&h).await.unwrap().unwrap();
        if let Object::Text(t) = got {
            assert_eq!(t.content.len(), content.len());
            assert_eq!(t.content, content);
        } else {
            panic!("expected Text");
        }
    }

    /// A comment containing newlines must roundtrip.
    pub async fn test_comment_with_newlines(&self) {
        let h = ContentHash::from_canonical(b"multiline_comment");
        let content = "line one\nline two\n\nline four".to_string();
        let mut tx = self.store.transaction().await.unwrap();
        tx.put(h, Object::Comment(CommentObject { content: content.clone() })).await.unwrap();
        tx.commit().await.unwrap();

        let got = self.store.get(&h).await.unwrap().unwrap();
        if let Object::Comment(c) = got {
            assert_eq!(c.content, content);
        } else {
            panic!("expected Comment");
        }
    }

    /// A processing instruction with no data field must roundtrip.
    pub async fn test_pi_no_data(&self) {
        let h = ContentHash::from_canonical(b"pi_nodata");
        let pi = PIObject { target: "bare-target".into(), data: None };
        let mut tx = self.store.transaction().await.unwrap();
        tx.put(h, Object::PI(pi.clone())).await.unwrap();
        tx.commit().await.unwrap();

        let got = self.store.get(&h).await.unwrap().unwrap();
        assert_eq!(got, Object::PI(pi));
    }

    /// An element with no children, no attributes, and no namespace must
    /// roundtrip.
    pub async fn test_element_zero_children(&self) {
        let h = ContentHash::from_canonical(b"empty_elem_full");
        let elem = ElementObject {
            local_name: "bare".into(),
            namespace_uri: None,
            namespace_prefix: None,
            extra_namespaces: vec![],
            attributes: vec![],
            children: vec![],
            inclusive_hash: h,
        };
        let mut tx = self.store.transaction().await.unwrap();
        tx.put(h, Object::Element(elem.clone())).await.unwrap();
        tx.commit().await.unwrap();

        let got = self.store.get(&h).await.unwrap().unwrap();
        assert_eq!(got, Object::Element(elem));
    }

    // ── Subtree consumer behavior (Cat E) ─────────────────────────────

    /// A deep linear chain of elements (depth 100) must walk to completion
    /// and yield exactly the expected count.
    pub async fn test_subtree_deep_chain(&self) {
        const DEPTH: usize = 100;

        let mut tx = self.store.transaction().await.unwrap();

        // Build leaf-to-root: each element references the previous.
        let leaf_text_hash = ContentHash::from_canonical(b"deep_leaf_text");
        tx.put(leaf_text_hash, Object::Text(TextObject { content: "leaf".into() })).await.unwrap();

        let mut prev_hash = leaf_text_hash;
        let mut hashes = vec![leaf_text_hash];
        for i in 0..DEPTH {
            let h = ContentHash::from_canonical(format!("deep_chain_{i}").as_bytes());
            let elem = ElementObject {
                local_name: "n".into(),
                namespace_uri: None,
                namespace_prefix: None,
                extra_namespaces: vec![],
                attributes: vec![],
                children: vec![prev_hash],
                inclusive_hash: h,
            };
            tx.put(h, Object::Element(elem)).await.unwrap();
            hashes.push(h);
            prev_hash = h;
        }

        let doc_hash = ContentHash::from_canonical(b"deep_chain_doc");
        let doc = DocumentObject { root: prev_hash, prologue: vec![] };
        tx.put(doc_hash, Object::Document(doc)).await.unwrap();
        hashes.push(doc_hash);
        tx.commit().await.unwrap();

        let pairs: Vec<(ContentHash, Object)> = self
            .store
            .subtree(&doc_hash)
            .map(|r| r.unwrap())
            .collect()
            .await;

        // doc + DEPTH elements + leaf text = DEPTH + 2
        assert_eq!(pairs.len(), DEPTH + 2);
    }

    /// An element with a thousand children must subtree-walk to completion.
    pub async fn test_subtree_wide_element(&self) {
        const WIDTH: usize = 1000;

        let mut tx = self.store.transaction().await.unwrap();

        let mut child_hashes = Vec::with_capacity(WIDTH);
        for i in 0..WIDTH {
            let h = ContentHash::from_canonical(format!("wide_child_{i}").as_bytes());
            tx.put(h, Object::Text(TextObject { content: format!("c{i}") })).await.unwrap();
            child_hashes.push(h);
        }

        let elem_hash = ContentHash::from_canonical(b"wide_elem");
        let elem = ElementObject {
            local_name: "root".into(),
            namespace_uri: None,
            namespace_prefix: None,
            extra_namespaces: vec![],
            attributes: vec![],
            children: child_hashes.clone(),
            inclusive_hash: elem_hash,
        };
        tx.put(elem_hash, Object::Element(elem)).await.unwrap();

        let doc_hash = ContentHash::from_canonical(b"wide_doc");
        tx.put(doc_hash, Object::Document(DocumentObject {
            root: elem_hash,
            prologue: vec![],
        })).await.unwrap();
        tx.commit().await.unwrap();

        let pairs: Vec<(ContentHash, Object)> = self
            .store
            .subtree(&doc_hash)
            .map(|r| r.unwrap())
            .collect()
            .await;

        // doc + element + WIDTH text children
        assert_eq!(pairs.len(), WIDTH + 2);
    }

    /// Dropping a subtree stream mid-walk must leave the store in a usable
    /// state for subsequent operations (no deadlock, no leaked locks).
    pub async fn test_subtree_consumer_drop_safe(&self) {
        // Build a small subtree.
        let text_hash = ContentHash::from_canonical(b"drop_safe_text");
        let elem_hash = ContentHash::from_canonical(b"drop_safe_elem");
        let doc_hash = ContentHash::from_canonical(b"drop_safe_doc");

        let mut tx = self.store.transaction().await.unwrap();
        tx.put(text_hash, Object::Text(TextObject { content: "hi".into() })).await.unwrap();
        tx.put(elem_hash, Object::Element(ElementObject {
            local_name: "r".into(),
            namespace_uri: None,
            namespace_prefix: None,
            extra_namespaces: vec![],
            attributes: vec![],
            children: vec![text_hash],
            inclusive_hash: elem_hash,
        })).await.unwrap();
        tx.put(doc_hash, Object::Document(DocumentObject {
            root: elem_hash,
            prologue: vec![],
        })).await.unwrap();
        tx.commit().await.unwrap();

        // Open and immediately drop a subtree stream.
        {
            let _stream = self.store.subtree(&doc_hash);
            // dropped here without consuming
        }

        // The store must remain functional: a fresh write/read works.
        let probe_hash = ContentHash::from_canonical(b"drop_safe_probe");
        let mut tx = self.store.transaction().await.unwrap();
        tx.put(probe_hash, Object::Text(TextObject { content: "still works".into() })).await.unwrap();
        tx.commit().await.unwrap();

        assert!(self.store.contains(&probe_hash).await.unwrap());
        // And re-reading the original subtree still works.
        let again: Vec<_> = self
            .store
            .subtree(&doc_hash)
            .map(|r| r.unwrap())
            .collect()
            .await;
        assert_eq!(again.len(), 3);
    }

    /// Taking only the first item from a subtree stream and dropping the
    /// rest must not corrupt subsequent full reads.
    pub async fn test_subtree_take_one_then_continue(&self) {
        let text_hash = ContentHash::from_canonical(b"take1_text");
        let elem_hash = ContentHash::from_canonical(b"take1_elem");
        let doc_hash = ContentHash::from_canonical(b"take1_doc");

        let mut tx = self.store.transaction().await.unwrap();
        tx.put(text_hash, Object::Text(TextObject { content: "x".into() })).await.unwrap();
        tx.put(elem_hash, Object::Element(ElementObject {
            local_name: "r".into(),
            namespace_uri: None,
            namespace_prefix: None,
            extra_namespaces: vec![],
            attributes: vec![],
            children: vec![text_hash],
            inclusive_hash: elem_hash,
        })).await.unwrap();
        tx.put(doc_hash, Object::Document(DocumentObject {
            root: elem_hash,
            prologue: vec![],
        })).await.unwrap();
        tx.commit().await.unwrap();

        {
            let mut stream = self.store.subtree(&doc_hash);
            let _first = stream.next().await;
            // stream dropped after taking 1 item
        }

        // Full re-read returns all 3.
        let all: Vec<_> = self
            .store
            .subtree(&doc_hash)
            .map(|r| r.unwrap())
            .collect()
            .await;
        assert_eq!(all.len(), 3);
    }
}
