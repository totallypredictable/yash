use crate::HashMap;

pub struct TrieNode {
    children: HashMap<char, Box<TrieNode>>,
    terminal: bool,
}

impl TrieNode {
    pub fn new() -> TrieNode {
        TrieNode {
            children: HashMap::new(),
            terminal: false,
        }
    }

    pub fn insert(&mut self, text: String) {
        let mut tmp: &mut TrieNode = self;
        for ch in (&text).chars() {
            if !tmp.children.contains_key(&ch) {
                tmp.children.insert(ch, Box::from(Self::new()));
            }

            tmp = &mut *tmp.children.get_mut(&ch).unwrap();
        }

        tmp.terminal = true;
    }

    pub fn search(&self, text: &str) -> Vec<String> {
        let mut tmp: &TrieNode = self;
        let mut results: Vec<String> = Vec::new();
        for ch in (&text).chars() {
            if tmp.children.contains_key(&ch) {
                tmp = &*tmp.children.get(&ch).unwrap();
            } else {
                return results;
            }
        }
        tmp.rec_search(&(text.to_string()), &mut results);

        return results;
    }

    fn rec_search(&self, text: &String, results: &mut Vec<String>) {
        if self.terminal {
            results.push(text.to_string());
        }
        for key in self.children.keys() {
            TrieNode::rec_search(
                &*self.children[key],
                &(text.to_string() + &key.to_string()),
                results,
            )
        }
    }
}
