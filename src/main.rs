use std::str;

// Store positions in packed (u32) form; this limits us to under 4GB of
// payload but makes the data structures a bit more compact.
struct PackedPos(u32);

impl PackedPos {
    fn from(pos: usize) -> PackedPos {
        assert!(pos <= std::u32::MAX as usize);
        PackedPos(pos as u32)
    }

    fn unpack(&self) -> usize {
        self.0 as usize
    }
}

// Leaves are treated separately from internal nodes, since the only
// data they actually have is their beginning character; they can be
// mostly implied and don't need an actual node representation.
#[derive(PartialEq)]
enum NodeRef {
    Leaf(usize),
    Inner(usize),
}

// Node references are also packed into u32s, which further limits us
// to about 2GB of payload and the equivalent number of internal nodes,
// but lets us pack the data more tightly.
#[derive(Clone, Copy)]
struct PackedRef(u32);

impl PackedRef {
    const MAX_IND : usize = (std::u32::MAX / 2) as usize;

    fn from_inner(ind: usize) -> Self {
        assert!(ind <= Self::MAX_IND);
        Self((ind*2 + 0) as u32)
    }

    fn from_leaf(pos: usize) -> Self {
        assert!(pos <= Self::MAX_IND);
        Self((pos*2 + 1) as u32)
    }
    
    fn unpack(&self) -> NodeRef {
        match self.0 & 1 {
            0 => NodeRef::Inner((self.0 >> 1) as usize),
            _ => NodeRef::Leaf((self.0 >> 1) as usize),
        }
    }

    fn is_none(&self) -> bool {
        self.0 == 0
    }
}

// An internal node in the suffix tree. Keeping a full array of 256
// child node references is a _terrible_ idea for memory use, and completely
// swamps the benefit of having PackedPos. Making PackedRefs smaller does help
// since this structure is essentially nothing but. Really though you would use
// a different representation for child links, classically: a linked list (ugh) or
// hash table. Another alternative is typical radix tree-style multiple node types
// depending on child count.
//
// We store the label of the incoming edge from the parent along with nodes, since
// every Node (save the root, which is special in other ways) has at least one
// incoming edge, because this is a tree.
struct Node {
    // The label of the incoming edge is payload[begin..end] (begin inclusive, end exclusive)
    begin: PackedPos,
    end: PackedPos,
    suffix: PackedRef, // suffix link
    child: [PackedRef; 256], // at most one child per possible character
}

impl Node {
    fn new_special(begin: usize, end: usize, suffix_ind: usize, child_ind: usize) -> Self {
        Node {
            begin: PackedPos::from(begin),
            end: PackedPos::from(end),
            suffix: PackedRef::from_inner(suffix_ind),
            child: [PackedRef::from_inner(child_ind); 256]
        }
    }

    fn new(begin: usize, end: usize, suffix_ind: usize) -> Self {
        Self::new_special(begin, end, suffix_ind, 0)
    }

    fn label_len(&self) -> usize {
        self.end.unpack() - self.begin.unpack()
    }
}

struct Cursor {
    node: usize,
    pos: usize,
}

struct SuffixTree<'a> {
    payload: &'a [u8],
    nodes: Vec<Node>,
}

impl<'a> SuffixTree<'a> {
    fn update(&mut self, cur_in: Cursor, new_end: usize) -> Cursor {
        let mut cur = cur_in;
        let new_ch = self.payload[new_end];
        let mut prev_insert_idx: usize = 0;

        loop {
            // Canonicalize active point
            while cur.pos < new_end {
                let link_ch = self.payload[cur.pos];
                match self.nodes[cur.node].child[link_ch as usize].unpack() {
                    NodeRef::Leaf(_) => {
                        // Leafs can absorb the entire rest of the string, and we can't
                        // descend into them; nothing to do.
                        break;
                    }
                    NodeRef::Inner(idx) => {
                        debug_assert!(idx != 0, "canonicalize should only follow real links");
                        let len = self.nodes[idx].label_len();
                        if len > new_end - cur.pos {
                            // Label of this inner node extends past the characters
                            // we currently have, so we're done!
                            break;
                        }

                        // Descend into this node and keep going
                        cur.node = idx;
                        cur.pos += len;
                    }
                }
            }

            // Do we have an outgoing link with the new character already?
            let insert_node_idx = if cur.pos == new_end {
                // Would insert right below active node; do we have
                // a link for this character already?
                if !self.nodes[cur.node].child[new_ch as usize].is_none() {
                    // We have this already; nothing to do for now!
                    break;
                } else {
                    // Insert right below current node.
                    cur.node
                }
            } else {
                // We're in the middle of a longer label; check whether we have a
                // mismatch (in which case we need to split) or not.

                // First character at the active point tells us which edge to
                // follow from the active node
                let edge_select_ch = self.payload[cur.pos] as usize;
                let edge_ref = self.nodes[cur.node].child[edge_select_ch];

                // For us to get here, this reference should exist
                debug_assert!(!edge_ref.is_none());

                let edge_label_begin = match edge_ref.unpack() {
                    NodeRef::Leaf(pos) => pos,
                    NodeRef::Inner(idx) => self.nodes[idx].begin.unpack()
                };
                let cur_label_pos = edge_label_begin + new_end - cur.pos;
                let cur_label_ch = self.payload[cur_label_pos];

                // Do we match the next character of the edge label or not?
                if new_ch == cur_label_ch {
                    // We do; nothing to do for now!
                    break;
                } else {
                    // We don't, so we need to split this edge
                    let mut n = Node::new(edge_label_begin, cur_label_pos, 1);
                    // Transfer over the existing node as first child
                    n.child[cur_label_ch as usize] = match edge_ref.unpack() {
                        NodeRef::Leaf(_) => PackedRef::from_leaf(cur_label_pos),
                        NodeRef::Inner(idx) => {
                            // Update the inner node to shorten its edge label
                            self.nodes[idx].begin = PackedPos::from(cur_label_pos);
                            PackedRef::from_inner(idx)
                        }
                    };
                    // Insert the new node and remember its index
                    let new_node_idx = self.nodes.len();
                    self.nodes.push(n);
                    // Link in the newly create node right below the active node
                    self.nodes[cur.node].child[edge_select_ch] = PackedRef::from_inner(new_node_idx);
                    // Return the index of the newly created node
                    new_node_idx
                }
            };

            // Update the suffix links
            self.nodes[prev_insert_idx].suffix = PackedRef::from_inner(insert_node_idx);
            prev_insert_idx = insert_node_idx;

            // Add the new leaf
            debug_assert!(self.nodes[insert_node_idx].child[new_ch as usize].is_none());
            self.nodes[insert_node_idx].child[new_ch as usize] = PackedRef::from_leaf(new_end);

            // Continue on to the next suffix
            if let NodeRef::Inner(idx) = self.nodes[cur.node].suffix.unpack() {
                cur.node = idx;
            } else {
                panic!("Suffix links must be to inner nodes!");
            }
        }

        cur
    }

    fn print_rec(&self, node: NodeRef, indent: usize, cur_end: usize) {
        print!("{:1$}", "", indent*2);
        match node {
            NodeRef::Inner(idx) => {
                let n = &self.nodes[idx];
                if idx == 1 {
                    println!("(root)");
                } else {
                    let suffix_ind = match n.suffix.unpack() {
                        NodeRef::Inner(sufidx) => sufidx,
                        _ => 0,
                    };
                    println!("\"{}\" (inner {}, suffix={})",
                        str::from_utf8(&self.payload[n.begin.unpack()..n.end.unpack()]).unwrap(),
                        idx, suffix_ind);
                }
                for r in n.child.iter() {
                    if !r.is_none() {
                        self.print_rec(r.unpack(), indent + 1, cur_end);
                    }
                }
            },
            NodeRef::Leaf(pos) => {
                println!("\"{}\" (leaf)", str::from_utf8(&self.payload[pos..cur_end]).unwrap());
            },
        }
    }

    fn print(&self) {
        self.print_rec(NodeRef::Inner(1), 0, self.payload.len());
    }

    fn from(payload: &'a [u8]) -> SuffixTree<'a> {
        let mut st = SuffixTree { payload: payload, nodes: Vec::new() };

        // Add the two sentinel nodes
        // Top is node 0. All child links point to the root.
        st.nodes.push(Node::new_special(0, 0, 0, 1));
        // Root is node 1; this is set up so traversing the link from top to the root consumes
        // exactly 1 (arbitrary) character.
        st.nodes.push(Node::new(0, 1, 0));

        // Update the suffix tree, adding the characters one by one
        (0..payload.len()).fold(Cursor { node: 1, pos: 0 }, |curs, pos| st.update(curs, pos));

        st
    }
}

fn main() {
    let payload = "bananas$".as_bytes();
    let st = SuffixTree::from(payload);
    st.print();
}
