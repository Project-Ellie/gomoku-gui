//! Persistent move tree and the cursor on its currently selected branch.
//!
//! Nodes are append-only during ordinary play. `line` is the selected path
//! from the root to its tip, and `moves` mirrors that path so `Game::moves`
//! can retain its slice-returning facade. The cursor is a ply in that line.

use engine::{Color, Move};

/// One node in the move tree. Node zero is the root and has no move.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HistoryNode {
    pub(crate) parent: Option<usize>,
    pub(crate) point: Option<Move>,
    pub(crate) children: Vec<usize>,
    pub(crate) current_child: Option<usize>,
    /// The side that resigned at this node, rather than the winner.
    pub(crate) resigned: Option<Color>,
}

/// A move tree plus the selected branch and review cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct History {
    nodes: Vec<HistoryNode>,
    line: Vec<usize>,
    moves: Vec<Move>,
    cursor: usize,
}

impl History {
    pub(crate) fn new() -> Self {
        Self {
            nodes: vec![HistoryNode {
                parent: None,
                point: None,
                children: Vec::new(),
                current_child: None,
                resigned: None,
            }],
            line: Vec::new(),
            moves: Vec::new(),
            cursor: 0,
        }
    }

    pub(crate) fn nodes(&self) -> &[HistoryNode] {
        &self.nodes
    }

    pub(crate) fn moves(&self) -> &[Move] {
        &self.moves
    }

    pub(crate) fn moves_with_siblings(&self) -> Vec<bool> {
        self.line
            .iter()
            .map(|&node| {
                let parent = self.nodes[node]
                    .parent
                    .expect("a selected move always has a parent");
                self.nodes[parent].children.len() > 1
            })
            .collect()
    }

    pub(crate) fn cursor(&self) -> usize {
        self.cursor
    }

    pub(crate) fn current_node(&self) -> usize {
        self.node_at(self.cursor)
    }

    pub(crate) fn tip_node(&self) -> usize {
        self.line.last().copied().unwrap_or(0)
    }

    pub(crate) fn resigned_at_tip(&self) -> Option<Color> {
        self.nodes[self.tip_node()].resigned
    }

    pub(crate) fn clear_resignation(&mut self) -> bool {
        let tip = self.tip_node();
        if self.nodes[tip].resigned.take().is_none() {
            return false;
        }
        self.cursor = self.line.len();
        true
    }

    pub(crate) fn resign_at_tip(&mut self, side: Color) {
        let tip = self.tip_node();
        self.nodes[tip].resigned = Some(side);
    }

    pub(crate) fn is_branch_point(&self) -> bool {
        self.nodes[self.current_node()].children.len() > 1
    }

    pub(crate) fn branch_options(&self) -> Vec<Move> {
        self.nodes[self.current_node()]
            .children
            .iter()
            .map(|&child| {
                self.nodes[child]
                    .point
                    .expect("a non-root child always has a move")
            })
            .collect()
    }

    pub(crate) fn selected_branch(&self) -> Option<usize> {
        let current = self.current_node();
        let selected = self.nodes[current].current_child?;
        self.nodes[current]
            .children
            .iter()
            .position(|&child| child == selected)
    }

    /// Change the remembered child and continuation without moving the cursor.
    pub(crate) fn set_current_child(&mut self, index: usize) -> bool {
        let current = self.current_node();
        let Some(&child) = self.nodes[current].children.get(index) else {
            return false;
        };
        if self.nodes[current].current_child == Some(child) {
            return false;
        }
        self.nodes[current].current_child = Some(child);
        self.line.truncate(self.cursor);
        self.moves.truncate(self.cursor);
        self.extend_selected_line();
        true
    }

    /// Select and enter a child, returning false for an invalid index.
    pub(crate) fn select_branch(&mut self, index: usize) -> Option<Move> {
        let current = self.current_node();
        let &child = self.nodes[current].children.get(index)?;
        self.nodes[current].current_child = Some(child);
        self.line.truncate(self.cursor);
        self.moves.truncate(self.cursor);
        let point = self.nodes[child]
            .point
            .expect("a non-root child always has a move");
        self.line.push(child);
        self.moves.push(point);
        self.cursor += 1;
        self.extend_selected_line();
        Some(point)
    }

    /// Follow an identical child or append a sibling, without deleting nodes.
    pub(crate) fn play(&mut self, point: Move) {
        let parent = self.current_node();
        let existing = self.nodes[parent]
            .children
            .iter()
            .copied()
            .find(|&child| self.nodes[child].point == Some(point));
        let child = existing.unwrap_or_else(|| {
            let id = self.nodes.len();
            self.nodes.push(HistoryNode {
                parent: Some(parent),
                point: Some(point),
                children: Vec::new(),
                current_child: None,
                resigned: None,
            });
            self.nodes[parent].children.push(id);
            id
        });
        self.nodes[parent].current_child = Some(child);
        self.line.truncate(self.cursor);
        self.moves.truncate(self.cursor);
        self.line.push(child);
        self.moves.push(point);
        self.cursor += 1;
        self.extend_selected_line();
    }

    /// Delete the selected branch's live tip and select a neighbouring child.
    /// Review cursors never delete: they must use `rewind` instead.
    pub(crate) fn delete_tip(&mut self) -> bool {
        if self.cursor != self.line.len() || self.line.is_empty() {
            return false;
        }
        let tip = self.tip_node();
        debug_assert!(self.nodes[tip].children.is_empty());
        let parent = self.nodes[tip]
            .parent
            .expect("a non-root tip always has a parent");
        let child_index = self.nodes[parent]
            .children
            .iter()
            .position(|&child| child == tip)
            .expect("the parent links to its selected tip");
        self.nodes[parent].children.remove(child_index);
        let sibling_count = self.nodes[parent].children.len();
        self.nodes[parent].current_child = self.nodes[parent]
            .children
            .get(child_index.min(sibling_count.saturating_sub(1)))
            .copied();

        self.nodes.remove(tip);
        let reindex = |id: usize| if id > tip { id - 1 } else { id };
        for node in &mut self.nodes {
            node.parent = node.parent.map(reindex);
            node.current_child = node.current_child.map(reindex);
            for child in &mut node.children {
                *child = reindex(*child);
            }
        }
        self.line.pop();
        self.moves.pop();
        self.cursor -= 1;
        for node in &mut self.line {
            *node = reindex(*node);
        }
        self.extend_selected_line();
        true
    }

    pub(crate) fn rewind(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        self.cursor -= 1;
        true
    }

    pub(crate) fn forward(&mut self) -> bool {
        if self.cursor == self.line.len() {
            return false;
        }
        self.cursor += 1;
        true
    }

    pub(crate) fn move_after_cursor(&self) -> Option<Move> {
        self.moves.get(self.cursor).copied()
    }

    pub(crate) fn path_to_node(&self, mut node: usize) -> Option<Vec<Move>> {
        if node >= self.nodes.len() {
            return None;
        }
        let mut path = Vec::new();
        while let Some(parent) = self.nodes[node].parent {
            path.push(self.nodes[node].point?);
            node = parent;
        }
        path.reverse();
        Some(path)
    }

    /// Restore a node in parent-before-child order, without following duplicates.
    pub(crate) fn restore_child(&mut self, parent: usize, point: Move) -> Option<usize> {
        if parent >= self.nodes.len()
            || self.nodes[parent]
                .children
                .iter()
                .any(|&child| self.nodes[child].point == Some(point))
        {
            return None;
        }
        let id = self.nodes.len();
        self.nodes.push(HistoryNode {
            parent: Some(parent),
            point: Some(point),
            children: Vec::new(),
            current_child: None,
            resigned: None,
        });
        self.nodes[parent].children.push(id);
        Some(id)
    }

    pub(crate) fn restore_current_child(&mut self, node: usize, child: Option<usize>) -> bool {
        if node >= self.nodes.len() {
            return false;
        }
        if let Some(child) = child {
            if !self.nodes[node].children.contains(&child) {
                return false;
            }
        }
        self.nodes[node].current_child = child;
        true
    }

    pub(crate) fn restore_resignation(&mut self, node: usize, side: Color) -> bool {
        if node >= self.nodes.len() {
            return false;
        }
        self.nodes[node].resigned = Some(side);
        true
    }

    /// Rebuild the selected line after all record nodes have been validated.
    pub(crate) fn finish_restore(&mut self) -> bool {
        self.line.clear();
        self.moves.clear();
        self.cursor = 0;
        let mut node = 0;
        while let Some(child) = self.nodes[node].current_child {
            let Some(point) = self.nodes[child].point else {
                return false;
            };
            self.line.push(child);
            self.moves.push(point);
            node = child;
        }
        self.cursor = self.line.len();
        true
    }

    fn node_at(&self, depth: usize) -> usize {
        if depth == 0 { 0 } else { self.line[depth - 1] }
    }

    fn extend_selected_line(&mut self) {
        let mut node = self.current_node();
        while let Some(child) = self.nodes[node].current_child {
            let point = self.nodes[child]
                .point
                .expect("a non-root child always has a move");
            self.line.push(child);
            self.moves.push(point);
            node = child;
        }
    }
}
