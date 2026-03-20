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
    };
}

#[cfg(test)]
pub(crate) use store_tests;

use std::collections::HashMap;

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
}
