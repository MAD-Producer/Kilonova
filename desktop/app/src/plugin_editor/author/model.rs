//! One bounded author history across layout, graph and source changes.
use motion_effects::native_layout::*;
use motion_graph::plugin_project::{PluginLogic, PluginProject};
use std::collections::VecDeque;

const HISTORY_BYTES: usize = 32 * 1024 * 1024;
struct Snapshot {
    project: PluginProject,
    bytes: usize,
}
impl Snapshot {
    fn new(project: PluginProject) -> Self {
        // No source bytes are logged. Serialization is bounded by validate_draft.
        let bytes = match serde_json::to_vec(&project) {
            Ok(value) => value.len(),
            Err(error) => {
                log::error!("plugin.author history size unavailable; retaining snapshot at full budget cause={}",crate::fallback_observation::error_cause(error));
                HISTORY_BYTES
            }
        };
        Self { project, bytes }
    }
}
pub struct Draft {
    pub project: PluginProject,
    pub revision: u64,
    saved: PluginProject,
    before: Option<PluginProject>,
    undo: VecDeque<Snapshot>,
    redo: Vec<Snapshot>,
}
impl Draft {
    pub fn new(project: PluginProject) -> Self {
        Self {
            saved: project.clone(),
            project,
            revision: 0,
            before: None,
            undo: VecDeque::new(),
            redo: Vec::new(),
        }
    }
    pub fn dirty(&self) -> bool {
        self.project != self.saved
    }
    pub fn active(&self) -> bool {
        self.before.is_some()
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn saved(&mut self, snapshot: PluginProject) {
        self.saved = snapshot;
    }
    pub fn begin(&mut self) {
        if self.before.is_none() {
            self.before = Some(self.project.clone());
        }
    }
    pub fn update(
        &mut self,
        edit: impl FnOnce(&mut PluginProject) -> Result<(), String>,
    ) -> Result<(), String> {
        let mut next = self.project.clone();
        edit(&mut next)?;
        next.validate_draft().map_err(|e| e.to_string())?;
        if next != self.project {
            self.begin();
            self.project = next;
            self.revision = self.revision.wrapping_add(1);
        }
        Ok(())
    }
    pub fn commit(&mut self) {
        if let Some(before) = self.before.take() {
            if before != self.project {
                if self.undo.len() == 128 {
                    self.undo.pop_front();
                }
                self.undo.push_back(Snapshot::new(before));
                self.trim_history();
                self.redo.clear();
            }
        }
    }
    pub fn cancel(&mut self) {
        if let Some(before) = self.before.take() {
            if before != self.project {
                self.project = before;
                self.revision = self.revision.wrapping_add(1);
            }
        }
    }
    pub fn undo(&mut self) {
        self.cancel();
        if let Some(p) = self.undo.pop_back() {
            self.redo.push(Snapshot::new(std::mem::replace(
                &mut self.project,
                p.project,
            )));
            self.revision = self.revision.wrapping_add(1);
        }
    }
    pub fn redo(&mut self) {
        self.cancel();
        if let Some(p) = self.redo.pop() {
            self.undo.push_back(Snapshot::new(std::mem::replace(
                &mut self.project,
                p.project,
            )));
            self.trim_history();
            self.revision = self.revision.wrapping_add(1);
        }
    }
    fn trim_history(&mut self) {
        let mut bytes: usize = self.undo.iter().map(|s| s.bytes).sum();
        while bytes > HISTORY_BYTES && self.undo.len() > 1 {
            if let Some(old) = self.undo.pop_front() {
                bytes = bytes.saturating_sub(old.bytes);
            }
        }
    }
    pub fn edit(
        &mut self,
        f: impl FnOnce(&mut PluginProject) -> Result<(), String>,
    ) -> Result<(), String> {
        self.commit();
        self.update(f)?;
        self.commit();
        Ok(())
    }
}
pub fn find<'a>(n: &'a LayoutNode, id: &str) -> Option<&'a LayoutNode> {
    if n.id == id {
        return Some(n);
    }
    n.children().find_map(|n| find(n, id))
}
pub fn find_mut<'a>(n: &'a mut LayoutNode, id: &str) -> Option<&'a mut LayoutNode> {
    if n.id == id {
        return Some(n);
    }
    match &mut n.content {
        LayoutContent::Row { children, .. }
        | LayoutContent::Column { children, .. }
        | LayoutContent::Group { children, .. } => {
            children.iter_mut().find_map(|n| find_mut(n, id))
        }
        LayoutContent::Tabs { tabs } => tabs.iter_mut().find_map(|t| find_mut(&mut t.child, id)),
        LayoutContent::Split { first, second, .. } => {
            find_mut(first, id).or_else(|| find_mut(second, id))
        }
        _ => None,
    }
}
pub fn collection(n: &mut LayoutNode) -> Option<&mut Vec<LayoutNode>> {
    match &mut n.content {
        LayoutContent::Row { children, .. }
        | LayoutContent::Column { children, .. }
        | LayoutContent::Group { children, .. } => Some(children),
        _ => None,
    }
}
fn remove(n: &mut LayoutNode, id: &str) -> Option<LayoutNode> {
    if let Some(children) = collection(n) {
        if let Some(i) = children.iter().position(|n| n.id == id) {
            return Some(children.remove(i));
        }
        return children.iter_mut().find_map(|n| remove(n, id));
    }
    // Fixed-arity split/tab roots cannot be removed; their descendants can.
    match &mut n.content {
        LayoutContent::Tabs { tabs } => tabs.iter_mut().find_map(|t| remove(&mut t.child, id)),
        LayoutContent::Split { first, second, .. } => {
            remove(first, id).or_else(|| remove(second, id))
        }
        _ => None,
    }
}
pub fn move_node(
    p: &mut PluginProject,
    id: &str,
    parent: &str,
    index: usize,
) -> Result<(), String> {
    let source = find(&p.interface.root, id).ok_or("layout source missing")?;
    if find(source, parent).is_some() {
        return Err("cannot move a component into itself or a descendant".into());
    }
    let n = remove(&mut p.interface.root, id)
        .ok_or("root, split slots and tab roots cannot be moved; move their children")?;
    let target =
        collection(find_mut(&mut p.interface.root, parent).ok_or("layout destination missing")?)
            .ok_or("drop into a row, column or group")?;
    target.insert(index.min(target.len()), n);
    Ok(())
}
pub fn delete(p: &mut PluginProject, id: &str) -> Result<(), String> {
    remove(&mut p.interface.root, id)
        .ok_or("cannot remove a fixed layout slot; remove its children instead")?;
    Ok(())
}
pub fn fresh_id(root: &LayoutNode, prefix: &str) -> String {
    for i in 1..=MAX_LAYOUT_NODES + 1 {
        let id = format!("{prefix}-{i}");
        fn used(n: &LayoutNode, id: &str) -> bool {
            (n.id == id || n.id.starts_with(&format!("{id}-")))
                || matches!(&n.content,LayoutContent::Tabs{tabs} if tabs.iter().any(|t|t.id==id))
                || n.children().any(|n| used(n, id))
        }
        if !used(root, &id) {
            return id;
        }
    }
    unreachable!("validated tree has at most MAX_LAYOUT_NODES identities")
}
#[derive(Clone)]
pub enum Component {
    Row,
    Column,
    Group,
    Tabs,
    Split,
    Preview,
    Note,
    Parameter(String),
}
pub fn insert(
    p: &mut PluginProject,
    parent: &str,
    index: usize,
    what: Component,
) -> Result<String, String> {
    let id = fresh_id(&p.interface.root, "component");
    let empty = |id: String| LayoutNode {
        id,
        width: LayoutExtent::Auto,
        height: LayoutExtent::Auto,
        content: LayoutContent::Column {
            gap: LayoutSpacing::None,
            align: LayoutAlign::Stretch,
            children: vec![],
        },
    };
    let content = match what {
        Component::Row => LayoutContent::Row {
            gap: LayoutSpacing::Compact,
            align: LayoutAlign::Stretch,
            children: vec![],
        },
        Component::Column => LayoutContent::Column {
            gap: LayoutSpacing::None,
            align: LayoutAlign::Stretch,
            children: vec![],
        },
        Component::Group => LayoutContent::Group {
            title: "分组 / Group".into(),
            collapsible: true,
            children: vec![],
        },
        Component::Tabs => LayoutContent::Tabs {
            tabs: vec![LayoutTab {
                id: format!("{id}-tab-1"),
                title: "标签页 / Tab".into(),
                child: empty(format!("{id}-page-1")),
            }],
        },
        Component::Split => LayoutContent::Split {
            axis: LayoutAxis::Horizontal,
            ratio: 0.5,
            first: Box::new(empty(format!("{id}-first"))),
            second: Box::new(empty(format!("{id}-second"))),
        },
        Component::Preview => LayoutContent::Preview,
        Component::Note => LayoutContent::Note {
            text: "说明 / Note".into(),
        },
        Component::Parameter(parameter) => LayoutContent::Parameter {
            parameter,
            control: ParameterControl::Auto,
        },
    };
    let node = LayoutNode {
        id: id.clone(),
        width: LayoutExtent::Auto,
        height: LayoutExtent::Auto,
        content,
    };
    let children =
        collection(find_mut(&mut p.interface.root, parent).ok_or("layout destination missing")?)
            .ok_or("insert into a row, column or group")?;
    children.insert(index.min(children.len()), node);
    Ok(id)
}
pub fn remove_parameter(p: &mut PluginProject, id: &str) -> Result<(), String> {
    fn reference(n: &LayoutNode, id: &str) -> bool {
        matches!(&n.content,LayoutContent::Parameter{parameter,..} if parameter==id)
            || n.children().any(|n| reference(n, id))
    }
    if reference(&p.interface.root, id) {
        return Err(
            "parameter is referenced by the layout; remove or rebind its controls first".into(),
        );
    }
    match &mut p.logic {
        PluginLogic::Wgsl{document}=>{
            if document.source.contains(&motion_graph::plugin_project::parameter_function(id)) { return Err("parameter accessor is referenced by WGSL; edit the source first".into()); }
            let i=document.parameters.iter().position(|v|v.id==id).ok_or("parameter missing")?;
            document.parameters.remove(i); Ok(())
        }
        PluginLogic::Graph{..}=>Err("edit public parameters in the node inspector; referenced graph parameters cannot be removed here".into()),
    }
}
