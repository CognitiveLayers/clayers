//! Porcelain API for repository operations.
//!
//! `Repo<S>` composes `ObjectStore + RefStore` to provide git-like
//! operations on XML Merkle DAGs.

use clayers_xml::ContentHash;
use chrono::Utc;

use crate::diff::{self, TreeDiff};
use crate::error::Result;
use crate::graph;
use crate::hash;
use crate::import;
use crate::export;
use crate::conflict::{self, ConflictInfo};
use crate::object::{Author, CommitObject, Object, TagObject, TreeEntry, TreeObject};
use crate::query::{self, QueryStore, QueryMode, QueryResult, NamespaceMap, RefQueryResult};
use crate::refs;
use crate::store::{ObjectStore, RefStore};

/// A repository with pluggable storage.
pub struct Repo<S: ObjectStore + RefStore + QueryStore> {
    store: S,
}

impl<S: ObjectStore + RefStore + QueryStore> Repo<S> {
    /// Create a new repository with the given store.
    #[must_use]
    pub fn init(store: S) -> Self {
        Self { store }
    }

    // --- Import/Export ---

    /// Import an XML string, decompose into the content-addressed store,
    /// and return the document hash.
    ///
    /// # Errors
    ///
    /// Returns an error if the XML is malformed or storage fails.
    pub async fn import_xml(&self, xml: &str) -> Result<ContentHash> {
        import::import_xml(&self.store, xml).await
    }

    /// Export a document from the store as a canonical XML string.
    ///
    /// # Errors
    ///
    /// Returns an error if the document is not found or reconstruction fails.
    pub async fn export_xml(&self, hash: ContentHash) -> Result<String> {
        export::export_xml(&self.store, hash).await
    }

    // --- Trees ---

    /// Build a tree object from file path -> document hash mappings.
    ///
    /// # Errors
    ///
    /// Returns an error if storage fails.
    pub async fn build_tree(
        &self,
        entries: Vec<(String, ContentHash)>,
    ) -> Result<ContentHash> {
        let tree_entries: Vec<TreeEntry> = entries
            .into_iter()
            .map(|(path, document)| TreeEntry { path, document })
            .collect();
        let tree = TreeObject::new(tree_entries);
        let xml = tree.to_xml();
        let tree_hash = hash::hash_exclusive(&xml)?;

        let mut tx = self.store.transaction().await?;
        tx.put(tree_hash, Object::Tree(tree)).await?;
        tx.commit().await?;

        Ok(tree_hash)
    }

    // --- Commits ---

    /// Create a commit on a branch.
    ///
    /// # Errors
    ///
    /// Returns an error if the tree is not found or storage fails.
    pub async fn commit(
        &self,
        branch: &str,
        tree: ContentHash,
        author: &Author,
        message: &str,
    ) -> Result<ContentHash> {
        // Get current branch tip as parent.
        let parents = match refs::get_branch(&self.store, branch).await? {
            Some(tip) => vec![tip],
            None => Vec::new(),
        };

        let commit = CommitObject {
            tree,
            parents,
            author: author.clone(),
            timestamp: Utc::now(),
            message: message.to_string(),
        };

        let xml = commit.to_xml();
        let commit_hash = hash::hash_exclusive(&xml)?;

        let mut tx = self.store.transaction().await?;
        tx.put(commit_hash, Object::Commit(commit)).await?;
        tx.commit().await?;

        // Update branch ref.
        self.store
            .set_ref(&refs::branch_ref(branch), commit_hash)
            .await?;

        // Update HEAD.
        refs::set_head(&self.store, commit_hash).await?;

        Ok(commit_hash)
    }

    // --- Branches ---

    /// Create a new branch pointing to a commit.
    ///
    /// # Errors
    ///
    /// Returns an error if the ref cannot be created.
    pub async fn create_branch(&self, name: &str, target: ContentHash) -> Result<()> {
        self.store
            .set_ref(&refs::branch_ref(name), target)
            .await
    }

    /// Delete a branch.
    ///
    /// # Errors
    ///
    /// Returns an error if the ref cannot be deleted.
    pub async fn delete_branch(&self, name: &str) -> Result<()> {
        self.store.delete_ref(&refs::branch_ref(name)).await
    }

    /// List all branches with their target commit hashes.
    ///
    /// # Errors
    ///
    /// Returns an error if refs cannot be listed.
    pub async fn list_branches(&self) -> Result<Vec<(String, ContentHash)>> {
        refs::list_branches(&self.store).await
    }

    // --- Tags ---

    /// Create an annotated tag.
    ///
    /// # Errors
    ///
    /// Returns an error if the tag cannot be created.
    pub async fn create_tag(
        &self,
        name: &str,
        target: ContentHash,
        tagger: &Author,
        message: &str,
    ) -> Result<()> {
        let tag = TagObject {
            target,
            name: name.to_string(),
            tagger: tagger.clone(),
            timestamp: Utc::now(),
            message: message.to_string(),
        };

        let xml = tag.to_xml();
        let tag_hash = hash::hash_exclusive(&xml)?;

        let mut tx = self.store.transaction().await?;
        tx.put(tag_hash, Object::Tag(tag)).await?;
        tx.commit().await?;

        self.store
            .set_ref(&refs::tag_ref(name), tag_hash)
            .await
    }

    /// List all tags with their target hashes.
    ///
    /// # Errors
    ///
    /// Returns an error if refs cannot be listed.
    pub async fn list_tags(&self) -> Result<Vec<(String, ContentHash)>> {
        refs::list_tags(&self.store).await
    }

    // --- History ---

    /// Walk commit history from a starting commit.
    ///
    /// # Errors
    ///
    /// Returns an error if commit objects cannot be loaded.
    pub async fn log(
        &self,
        from: ContentHash,
        limit: Option<usize>,
    ) -> Result<Vec<CommitObject>> {
        let history = graph::walk_history(&self.store, from, limit).await?;
        Ok(history.into_iter().map(|(_, commit)| commit).collect())
    }

    // --- Diff ---

    /// Compute a structural diff between two content-addressed trees.
    ///
    /// # Errors
    ///
    /// Returns an error if objects cannot be loaded.
    pub async fn diff(&self, a: ContentHash, b: ContentHash) -> Result<TreeDiff> {
        diff::diff(&self.store, a, b).await
    }

    // --- File-level diff ---

    /// Compare two tree objects and return file-level changes.
    ///
    /// # Errors
    ///
    /// Returns an error if tree objects cannot be loaded.
    pub async fn diff_trees(
        &self,
        tree_hash_a: ContentHash,
        tree_hash_b: ContentHash,
    ) -> Result<Vec<diff::FileChange>> {
        let obj_a = self
            .store
            .get(&tree_hash_a)
            .await?
            .ok_or(crate::error::Error::NotFound(tree_hash_a))?;
        let obj_b = self
            .store
            .get(&tree_hash_b)
            .await?
            .ok_or(crate::error::Error::NotFound(tree_hash_b))?;

        let Object::Tree(ta) = obj_a else {
            return Err(crate::error::Error::InvalidObject(
                "expected Tree object".into(),
            ));
        };
        let Object::Tree(tb) = obj_b else {
            return Err(crate::error::Error::InvalidObject(
                "expected Tree object".into(),
            ));
        };

        Ok(diff::diff_trees(&ta, &tb))
    }

    /// Export two documents and diff them as XML strings.
    ///
    /// # Errors
    ///
    /// Returns an error if documents cannot be exported or diff fails.
    pub async fn diff_file(
        &self,
        doc_a: ContentHash,
        doc_b: ContentHash,
    ) -> Result<clayers_xml::XmlDiff> {
        let xml_a = self.export_xml(doc_a).await?;
        let xml_b = self.export_xml(doc_b).await?;
        clayers_xml::diff_xml(&xml_a, &xml_b).map_err(|e| {
            crate::error::Error::InvalidObject(format!("XML diff failed: {e}"))
        })
    }

    // --- Conflict detection ---

    /// Check whether a document contains unresolved conflicts.
    ///
    /// # Errors
    ///
    /// Returns an error if objects cannot be loaded.
    pub async fn has_conflicts(&self, document: ContentHash) -> Result<bool> {
        conflict::has_conflicts(&self.store, document).await
    }

    /// List all unresolved conflicts in a document.
    ///
    /// # Errors
    ///
    /// Returns an error if objects cannot be loaded.
    pub async fn list_conflicts(&self, document: ContentHash) -> Result<Vec<ConflictInfo>> {
        conflict::list_conflicts(&self.store, document).await
    }

    // --- Query ---

    /// Query a document or revision with an `XPath` expression.
    ///
    /// # Errors
    ///
    /// Returns an error if the revspec cannot be resolved or the query fails.
    pub async fn query(
        &self,
        revspec: &str,
        xpath: &str,
        mode: QueryMode,
        namespaces: &NamespaceMap,
    ) -> Result<QueryResult> {
        query::query(&self.store, &self.store, &self.store, revspec, xpath, mode, namespaces).await
    }

    /// Query each document in the tree, returning per-document results.
    ///
    /// When `files` is non-empty, only matching documents are queried.
    ///
    /// # Errors
    ///
    /// Returns an error if resolution or query fails.
    pub async fn query_by_document(
        &self,
        revspec: &str,
        xpath: &str,
        mode: QueryMode,
        namespaces: &NamespaceMap,
        files: &[String],
    ) -> Result<Vec<query::DocumentQueryResult>> {
        query::query_by_document(
            &self.store, &self.store, &self.store, revspec, xpath, mode, namespaces, files,
        )
        .await
    }

    /// Query across all refs matching a prefix.
    ///
    /// # Errors
    ///
    /// Returns an error if refs cannot be listed or queries fail.
    pub async fn query_refs(
        &self,
        prefix: &str,
        xpath: &str,
        mode: QueryMode,
        namespaces: &NamespaceMap,
    ) -> Result<Vec<RefQueryResult>> {
        query::query_refs(&self.store, &self.store, &self.store, prefix, xpath, mode, namespaces).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::memory::MemoryStore;

    #[tokio::test]
    async fn init_import_commit_export_roundtrip() {
        let store = MemoryStore::new();
        let repo = Repo::init(store);

        let xml = "<root>hello</root>";
        let doc_hash = repo.import_xml(xml).await.unwrap();
        let tree_hash = repo
            .build_tree(vec![("doc.xml".into(), doc_hash)])
            .await
            .unwrap();

        let author = Author {
            name: "Alice".into(),
            email: "alice@example.com".into(),
        };
        let commit_hash = repo
            .commit("main", tree_hash, &author, "Initial commit")
            .await
            .unwrap();

        // Verify branch was created.
        let branches = repo.list_branches().await.unwrap();
        assert!(branches.iter().any(|(name, _)| name == "main"));

        // Verify commit is in history.
        let history = repo.log(commit_hash, None).await.unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].message, "Initial commit");
    }

    #[tokio::test]
    async fn build_tree_stores_object() {
        let store = MemoryStore::new();
        let repo = Repo::init(store);
        let doc_hash = repo.import_xml("<r>x</r>").await.unwrap();
        let tree_hash = repo
            .build_tree(vec![("a.xml".into(), doc_hash)])
            .await
            .unwrap();
        let exported = repo.export_xml(doc_hash).await.unwrap();
        assert!(exported.contains('x'));
        // Tree object should exist in store.
        assert!(tree_hash != doc_hash);
    }

    #[tokio::test]
    async fn build_tree_sorts() {
        let store = MemoryStore::new();
        let repo = Repo::init(store);
        let h1 = repo.import_xml("<a/>").await.unwrap();
        let h2 = repo.import_xml("<b/>").await.unwrap();
        // Input in reverse order, tree should sort.
        let t1 = repo.build_tree(vec![
            ("z.xml".into(), h1),
            ("a.xml".into(), h2),
        ]).await.unwrap();
        let t2 = repo.build_tree(vec![
            ("a.xml".into(), h2),
            ("z.xml".into(), h1),
        ]).await.unwrap();
        assert_eq!(t1, t2, "same entries in different order should produce same tree hash");
    }

    #[tokio::test]
    async fn multi_file_commit_roundtrip() {
        let store = MemoryStore::new();
        let repo = Repo::init(store);
        let h1 = repo.import_xml("<a>one</a>").await.unwrap();
        let h2 = repo.import_xml("<b>two</b>").await.unwrap();
        let h3 = repo.import_xml("<c>three</c>").await.unwrap();
        let tree_hash = repo.build_tree(vec![
            ("a.xml".into(), h1),
            ("b.xml".into(), h2),
            ("c.xml".into(), h3),
        ]).await.unwrap();
        let author = Author { name: "T".into(), email: "t@t".into() };
        let commit_hash = repo.commit("main", tree_hash, &author, "multi").await.unwrap();
        let history = repo.log(commit_hash, None).await.unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].message, "multi");
    }

    #[tokio::test]
    async fn create_and_delete_branch() {
        let store = MemoryStore::new();
        let repo = Repo::init(store);
        let h = ContentHash::from_canonical(b"target");

        repo.create_branch("feature", h).await.unwrap();
        let branches = repo.list_branches().await.unwrap();
        assert!(branches.iter().any(|(n, _)| n == "feature"));

        repo.delete_branch("feature").await.unwrap();
        let branches = repo.list_branches().await.unwrap();
        assert!(!branches.iter().any(|(n, _)| n == "feature"));
    }

    #[tokio::test]
    async fn create_and_list_tags() {
        let store = MemoryStore::new();
        let repo = Repo::init(store);
        let h = ContentHash::from_canonical(b"target");

        let tagger = Author {
            name: "Bob".into(),
            email: "bob@example.com".into(),
        };
        repo.create_tag("v1.0", h, &tagger, "Release v1.0")
            .await
            .unwrap();

        let tags = repo.list_tags().await.unwrap();
        assert!(tags.iter().any(|(n, _)| n == "v1.0"));
    }

    #[tokio::test]
    async fn diff_identical_documents() {
        let store = MemoryStore::new();
        let repo = Repo::init(store);
        let h = ContentHash::from_canonical(b"same");
        let d = repo.diff(h, h).await.unwrap();
        assert!(d.is_empty());
    }
}
