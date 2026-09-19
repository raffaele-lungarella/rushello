//! A character-based trie for storing words and finding prefix completions.
//!
//! Characters label the edges between nodes. For `git` and `go`, the root has
//! a `g` child, whose children are `i` and `o`; the `i` node has a `t` child.
//! The nodes reached through `t` and `o` mark complete words.
//!
//! Matching is case-sensitive and uses Rust `char` values without Unicode
//! normalization. Children are stored in vectors, so each child lookup is a
//! linear scan rather than a hash lookup.

/// A trie with insertion and prefix lookup. `Default` creates an empty trie.
#[derive(Default)]
pub struct Trie {
    /// The empty path, with no incoming character. Its word marker represents `""`.
    pub root: TrieNode,
}

impl Trie {
    /// Stores a word without duplicating existing paths or word markers.
    ///
    /// Inserting a word that is a prefix of another preserves both words.
    /// Inserting an empty string marks the root as a word.
    pub fn insert(&mut self, word: &str) {
        self.root.insert(word);
    }

    /// Returns full stored words beginning with `prefix`, not just their suffixes.
    ///
    /// Includes `prefix` itself if it is stored. An empty prefix returns all words;
    /// a missing prefix returns an empty vector. For example, after inserting
    /// `cargo` and `cargo-clippy`, prefix `cargo` returns both words.
    ///
    /// Results follow depth-first traversal with children visited in their
    /// insertion order, not lexicographic order. Sort the result if needed.
    pub fn words_with_prefix(&self, prefix: &str) -> Vec<String> {
        self.root.words_with_prefix(prefix)
    }
}

/// A node representing the path of characters used to reach it.
#[derive(Default)]
pub struct TrieNode {
    /// Each pair labels an edge with a character and stores its child node.
    /// Insertion maintains at most one child per character.
    pub children: Vec<(char, TrieNode)>,
    /// Whether the path to this node is a stored word, even if it has children.
    pub is_word: bool,
}

impl TrieNode {
    /// Inserts a character path starting at this node, reusing shared prefixes.
    fn insert(&mut self, word: &str) {
        let mut current = self;

        for ch in word.chars() {
            if let Some(index) = current.children.iter().position(|node| node.0 == ch) {
                current = &mut current.children[index].1;
            } else {
                current.children.push((ch, TrieNode::default()));
                let (_, child) = current.children.last_mut().unwrap();
                current = child;
            }
        }
        current.is_word = true;
    }

    /// Finds the prefix node, then collects complete words in its subtree.
    fn words_with_prefix(&self, prefix: &str) -> Vec<String> {
        let mut current = self;

        // Only collect words after matching the entire prefix.
        for ch in prefix.chars() {
            if let Some(matching_index) = current.children.iter().position(|node| node.0 == ch) {
                current = &current.children[matching_index].1;
            } else {
                return Vec::new();
            }
        }

        let mut words = vec![];
        let mut accumulator = prefix.to_owned();

        current.collect_words(&mut accumulator, &mut words);
        words
    }

    /// Collects words depth-first, including this node if it ends a word.
    ///
    /// `current_word` must contain the path to this node on entry. It is restored
    /// before returning; `words` keeps the completed strings found along the way.
    /// For prefix `g`, collection starts at the `g` node with `current_word = "g"`,
    /// and the child loop explores `i` and `o` for the words `git` and `go`.
    fn collect_words(&self, current_word: &mut String, words: &mut Vec<String>) {
        if self.is_word {
            // Keep a snapshot because the shared accumulator changes during traversal.
            words.push(current_word.clone());
        }

        for (character, child) in &self.children {
            current_word.push(*character);
            child.collect_words(current_word, words);
            // Restore the path before exploring the next sibling.
            current_word.pop();
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::trie::Trie;

    fn cargo_trie() -> Trie {
        let mut trie = Trie::default();
        trie.insert("cargo");
        trie.insert("cargo-clippy");
        trie
    }

    #[test]
    fn prefix_search_returns_full_words() {
        let trie = cargo_trie();
        let mut words = trie.words_with_prefix("car");
        words.sort();
        assert_eq!(words, ["cargo", "cargo-clippy"]);
    }

    #[test]
    fn prefix_search_includes_prefix_when_it_is_a_word() {
        for commands in [["cargo", "cargo-clippy"], ["cargo-clippy", "cargo"]] {
            let mut trie = Trie::default();
            for command in commands {
                trie.insert(command);
            }
            let mut words = trie.words_with_prefix("cargo");
            words.sort();
            assert_eq!(words, ["cargo", "cargo-clippy"]);
        }
    }

    #[test]
    fn prefix_search_returns_only_matching_descendants() {
        let trie = cargo_trie();
        assert_eq!(trie.words_with_prefix("cargo-"), ["cargo-clippy"]);
    }

    #[test]
    fn prefix_search_finds_an_exact_leaf_word() {
        let trie = cargo_trie();
        assert_eq!(trie.words_with_prefix("cargo-clippy"), ["cargo-clippy"]);
    }

    #[test]
    fn prefix_search_rejects_missing_characters() {
        let trie = cargo_trie();
        for prefix in ["git", "xcargo", "carxgo", "cargox", "cargo-clippy-extra"] {
            assert!(
                trie.words_with_prefix(prefix).is_empty(),
                "unexpected match for {prefix}"
            );
        }
    }

    #[test]
    fn empty_prefix_returns_all_words() {
        let trie = cargo_trie();
        let mut words = trie.words_with_prefix("");
        words.sort();
        assert_eq!(words, ["cargo", "cargo-clippy"]);
    }

    #[test]
    fn prefix_search_on_empty_trie_returns_no_words() {
        let trie = Trie::default();
        assert!(trie.words_with_prefix("cargo").is_empty());
        assert!(trie.words_with_prefix("").is_empty());
    }

    #[test]
    fn inserts_git_as_a_chain() {
        let mut trie = Trie::default();

        trie.insert("git");

        let mut node = &trie.root;
        for expected in ['g', 'i', 't'] {
            assert_eq!(node.children.len(), 1);
            let (character, child) = node.children.first().unwrap();
            assert_eq!(*character, expected);
            node = child;
        }
        assert!(node.children.is_empty());
    }

    #[test]
    fn marks_word_endings_in_either_insertion_order() {
        for words in [["git", "github"], ["github", "git"]] {
            let mut trie = Trie::default();
            for word in words {
                trie.insert(word);
            }

            assert!(!trie.root.is_word);
            let mut node = &trie.root;
            for (index, character) in "github".chars().enumerate() {
                let (_, child) = node
                    .children
                    .iter()
                    .find(|child| child.0 == character)
                    .unwrap();
                node = child;
                assert_eq!(
                    node.is_word,
                    index == 2 || index == 5,
                    "unexpected word marker at {character}"
                );
            }
        }
    }

    #[test]
    fn inserts_go_with_git_already_present() {
        let mut trie = Trie::default();

        trie.insert("git");
        trie.insert("go");

        let root = &trie.root;
        assert_eq!(
            root.children
                .iter()
                .map(|child| child.0)
                .collect::<Vec<_>>(),
            ['g']
        );

        let g = &root.children[0].1;
        assert_eq!(
            g.children.iter().map(|child| child.0).collect::<Vec<_>>(),
            ['i', 'o']
        );

        let i = &g.children[0].1;
        assert_eq!(
            i.children.iter().map(|child| child.0).collect::<Vec<_>>(),
            ['t']
        );
        assert!(i.children[0].1.children.is_empty());

        let o = &g.children[1].1;
        assert!(o.children.is_empty());
    }
}
