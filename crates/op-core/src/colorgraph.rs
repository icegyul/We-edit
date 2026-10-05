//! The color node graph: the data behind the node-based grade (a clip's `Color Nodes` effect).
//!
//! A graph connects a **Source** (the clip's picture) through nodes to an **Output**:
//!
//! - a **Corrector** grades what comes in: primaries (lift, gamma, gain, offset, contrast, pivot,
//!   saturation, hue, temperature, tint, shadows, highlights, color boost), custom curves
//!   (master, red, green, blue, hue against hue, hue against saturation, luminance against
//!   saturation, saturation against saturation), a LUT, a **qualifier** (a key from hue,
//!   saturation and luminance ranges), **windows** (shapes) and a key that limits where the grade
//!   shows, plus blur and sharpen;
//! - a **Parallel mixer** blends the pictures of several nodes that all started from the same
//!   input;
//! - a **Layer mixer** stacks pictures, each through its node's key.
//!
//! Every node has one picture output and one key output (the matte of its qualifier and windows),
//! and a corrector can take a key as input (inverted: an "outside" node). Graphs are stored as
//! JSON in the effect's `graph` parameter.

use serde::{Deserialize, Serialize};

pub type NodeId = u32;

/// A point of a curve, `[x, y]`.
pub type Pt = [f32; 2];

fn one() -> f32 {
    1.0
}

fn yes() -> bool {
    true
}

// ------------------------------------------------------------------------------------ grade

/// Primaries. The four wheels hold `[r, g, b, master]`: the color offsets are around 0 and the
/// master is a brightness change of that range.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Grade {
    pub lift: [f32; 4],
    pub gamma: [f32; 4],
    pub gain: [f32; 4],
    pub offset: [f32; 4],
    pub contrast: f32,
    pub pivot: f32,
    /// Saturation as a factor (1 is unchanged).
    pub saturation: f32,
    /// Hue rotation in degrees.
    pub hue: f32,
    pub temperature: f32,
    pub tint: f32,
    pub shadows: f32,
    pub highlights: f32,
    /// Saturates the dull colors more than the vivid ones, 0..100.
    pub boost: f32,
}

impl Default for Grade {
    fn default() -> Self {
        Grade {
            lift: [0.0; 4],
            gamma: [0.0; 4],
            gain: [0.0; 4],
            offset: [0.0; 4],
            contrast: 1.0,
            pivot: 0.435,
            saturation: 1.0,
            hue: 0.0,
            temperature: 0.0,
            tint: 0.0,
            shadows: 0.0,
            highlights: 0.0,
            boost: 0.0,
        }
    }
}

impl Grade {
    pub fn is_neutral(&self) -> bool {
        *self == Grade::default() || {
            let d = Grade::default();
            let same =
                |a: &[f32; 4], b: &[f32; 4]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5);
            same(&self.lift, &d.lift)
                && same(&self.gamma, &d.gamma)
                && same(&self.gain, &d.gain)
                && same(&self.offset, &d.offset)
                && (self.contrast - 1.0).abs() < 1e-5
                && (self.saturation - 1.0).abs() < 1e-5
                && self.hue.abs() < 1e-4
                && self.temperature.abs() < 1e-4
                && self.tint.abs() < 1e-4
                && self.shadows.abs() < 1e-4
                && self.highlights.abs() < 1e-4
                && self.boost.abs() < 1e-4
        }
    }
}

/// Custom curves. Master, red, green and blue map a value to a value; the others bend a color
/// property by a color property and are `0.5` where nothing changes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Curves {
    pub master: Vec<Pt>,
    pub red: Vec<Pt>,
    pub green: Vec<Pt>,
    pub blue: Vec<Pt>,
    pub hue_vs_hue: Vec<Pt>,
    pub hue_vs_sat: Vec<Pt>,
    pub lum_vs_sat: Vec<Pt>,
    pub sat_vs_sat: Vec<Pt>,
}

pub fn straight() -> Vec<Pt> {
    vec![[0.0, 0.0], [1.0, 1.0]]
}

pub fn flat() -> Vec<Pt> {
    vec![[0.0, 0.5], [1.0, 0.5]]
}

impl Default for Curves {
    fn default() -> Self {
        Curves {
            master: straight(),
            red: straight(),
            green: straight(),
            blue: straight(),
            hue_vs_hue: flat(),
            hue_vs_sat: flat(),
            lum_vs_sat: flat(),
            sat_vs_sat: flat(),
        }
    }
}

impl Curves {
    fn is(c: &[Pt], target: &[Pt]) -> bool {
        c.is_empty() || c == target
    }

    pub fn rgb_is_neutral(&self) -> bool {
        let s = straight();
        Self::is(&self.master, &s)
            && Self::is(&self.red, &s)
            && Self::is(&self.green, &s)
            && Self::is(&self.blue, &s)
    }

    pub fn hsl_is_neutral(&self) -> bool {
        let f = flat();
        Self::is(&self.hue_vs_hue, &f)
            && Self::is(&self.hue_vs_sat, &f)
            && Self::is(&self.lum_vs_sat, &f)
            && Self::is(&self.sat_vs_sat, &f)
    }
}

// ------------------------------------------------------------------------- qualifier, windows

/// One range of a qualifier: values from `low` to `high` are selected, and fade out over the two
/// softness widths outside them.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Band {
    pub low: f32,
    pub high: f32,
    pub low_soft: f32,
    pub high_soft: f32,
}

impl Band {
    pub const ALL: Band = Band {
        low: 0.0,
        high: 1.0,
        low_soft: 0.0,
        high_soft: 0.0,
    };
}

/// Selects pixels by color: hue (a range around a center, which wraps around the wheel),
/// saturation and luminance.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Qualifier {
    pub enabled: bool,
    /// Hue range around `hue_center`, 0..1 of the wheel.
    pub hue_center: f32,
    pub hue_width: f32,
    pub hue_soft: f32,
    pub use_hue: bool,
    pub sat: Band,
    pub lum: Band,
    pub invert: bool,
    /// Softens the matte, in pixels at 1080p.
    pub blur: f32,
    /// The matte below this becomes 0 and above `clean_white` becomes 1.
    pub clean_black: f32,
    pub clean_white: f32,
}

impl Default for Qualifier {
    fn default() -> Self {
        Qualifier {
            enabled: false,
            hue_center: 0.0,
            hue_width: 0.1,
            hue_soft: 0.05,
            use_hue: true,
            sat: Band::ALL,
            lum: Band::ALL,
            invert: false,
            blur: 0.0,
            clean_black: 0.0,
            clean_white: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WindowKind {
    Rectangle,
    Ellipse,
    /// A gradient across the frame: full on one side, none on the other.
    Linear,
    Polygon,
}

/// A shape: part of the matte of a node.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Window {
    pub kind: WindowKind,
    pub enabled: bool,
    /// Center, in 0..1 of the frame.
    pub center: [f32; 2],
    /// Width and height, in 0..1 of the frame (the length of the gradient for Linear).
    pub size: [f32; 2],
    /// Degrees.
    pub rotation: f32,
    /// How far the edge fades, 0..1 of the smaller side.
    pub softness: f32,
    /// What is outside the shape is selected instead.
    pub invert: bool,
    /// Corners of a Polygon, in 0..1 of the frame.
    pub points: Vec<Pt>,
}

impl Default for Window {
    fn default() -> Self {
        Window {
            kind: WindowKind::Ellipse,
            enabled: true,
            center: [0.5, 0.5],
            size: [0.4, 0.4],
            rotation: 0.0,
            softness: 0.1,
            invert: false,
            points: Vec::new(),
        }
    }
}

impl Window {
    pub fn new(kind: WindowKind) -> Window {
        let mut w = Window {
            kind,
            ..Default::default()
        };
        match kind {
            WindowKind::Rectangle => w.size = [0.5, 0.4],
            WindowKind::Linear => {
                w.size = [0.0, 0.5];
                w.softness = 0.5;
            }
            WindowKind::Polygon => {
                w.points = vec![[0.3, 0.3], [0.7, 0.3], [0.7, 0.7], [0.3, 0.7]];
            }
            WindowKind::Ellipse => {}
        }
        w
    }
}

/// How the matte of a node is adjusted before it limits the grade.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct KeyAdjust {
    pub gain: f32,
    pub offset: f32,
    pub blur: f32,
    pub invert: bool,
}

impl Default for KeyAdjust {
    fn default() -> Self {
        KeyAdjust {
            gain: 1.0,
            offset: 0.0,
            blur: 0.0,
            invert: false,
        }
    }
}

impl KeyAdjust {
    pub fn is_neutral(&self) -> bool {
        *self == KeyAdjust::default()
    }
}

// ----------------------------------------------------------------------------------- nodes

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Corrector {
    pub grade: Grade,
    pub curves: Curves,
    pub qualifier: Qualifier,
    pub windows: Vec<Window>,
    pub key: KeyAdjust,
    /// A 3D LUT file (.cube) applied after the grade, and how much of it.
    pub lut: String,
    #[serde(default = "one")]
    pub lut_amount: f32,
    pub blur: f32,
    pub sharpen: f32,
    /// A key coming into this node limits it inverted (an "outside" node).
    pub key_in_invert: bool,
    /// How much of the grade shows, 0..1.
    #[serde(default = "one")]
    pub amount: f32,
}

impl Default for Corrector {
    fn default() -> Self {
        Corrector {
            grade: Grade::default(),
            curves: Curves::default(),
            qualifier: Qualifier::default(),
            windows: Vec::new(),
            key: KeyAdjust::default(),
            lut: String::new(),
            lut_amount: 1.0,
            blur: 0.0,
            sharpen: 0.0,
            key_in_invert: false,
            amount: 1.0,
        }
    }
}

impl Corrector {
    /// Nothing graded, keyed or filtered: the picture passes through.
    pub fn is_neutral(&self) -> bool {
        self.grade.is_neutral()
            && self.curves.rgb_is_neutral()
            && self.curves.hsl_is_neutral()
            && self.lut.is_empty()
            && self.blur.abs() < 1e-4
            && self.sharpen.abs() < 1e-4
    }

    /// Whether a matte limits the node: a qualifier or an enabled window.
    pub fn has_matte(&self) -> bool {
        self.qualifier.enabled || self.windows.iter().any(|w| w.enabled)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Blend {
    #[default]
    Normal,
    Add,
    Screen,
    Multiply,
    Overlay,
    Difference,
}

impl Blend {
    pub const ALL: [Blend; 6] = [
        Blend::Normal,
        Blend::Add,
        Blend::Screen,
        Blend::Multiply,
        Blend::Overlay,
        Blend::Difference,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Blend::Normal => "Normal",
            Blend::Add => "Add",
            Blend::Screen => "Screen",
            Blend::Multiply => "Multiply",
            Blend::Overlay => "Overlay",
            Blend::Difference => "Difference",
        }
    }

    pub fn index(self) -> u32 {
        self as u32
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Mixer {
    pub blend: Blend,
    /// Weight (parallel) or opacity (layer) of each input, in order of the input sockets.
    pub weights: Vec<f32>,
}

impl Default for Mixer {
    fn default() -> Self {
        Mixer {
            blend: Blend::Normal,
            weights: Vec::new(),
        }
    }
}

impl Mixer {
    pub fn weight(&self, i: usize) -> f32 {
        self.weights.get(i).copied().unwrap_or(1.0)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum NodeKind {
    Source,
    Output,
    Corrector(Box<Corrector>),
    Parallel(Mixer),
    Layer(Mixer),
}

impl NodeKind {
    pub fn name(&self) -> &'static str {
        match self {
            NodeKind::Source => "Source",
            NodeKind::Output => "Output",
            NodeKind::Corrector(_) => "Corrector",
            NodeKind::Parallel(_) => "Parallel Mixer",
            NodeKind::Layer(_) => "Layer Mixer",
        }
    }

    /// Takes any number of picture inputs.
    pub fn is_mixer(&self) -> bool {
        matches!(self, NodeKind::Parallel(_) | NodeKind::Layer(_))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    #[serde(default)]
    pub label: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Place in the node editor.
    #[serde(default)]
    pub pos: [f32; 2],
    pub kind: NodeKind,
}

/// A connection: the picture (or, with `key`, the key) of `from` goes into input `input` of `to`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edge {
    pub from: NodeId,
    pub to: NodeId,
    #[serde(default)]
    pub input: u8,
    #[serde(default)]
    pub key: bool,
}

/// What the viewer shows instead of the result.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum View {
    /// The matte of a node as black and white.
    Matte(NodeId),
    /// The picture with what the node does not select dimmed.
    Highlight(NodeId),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ColorGraph {
    pub version: u32,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    #[serde(default)]
    pub view: Option<View>,
}

pub const SOURCE: NodeId = 0;
pub const OUTPUT: NodeId = 1;

impl Default for ColorGraph {
    /// Source, one corrector, Output, in a row.
    fn default() -> Self {
        let mut g = ColorGraph::empty();
        let id = g.add(NodeKind::Corrector(Box::default()), [160.0, 0.0]);
        g.edges = vec![
            Edge {
                from: SOURCE,
                to: id,
                input: 0,
                key: false,
            },
            Edge {
                from: id,
                to: OUTPUT,
                input: 0,
                key: false,
            },
        ];
        g
    }
}

impl ColorGraph {
    /// Source and Output, not connected.
    pub fn empty() -> ColorGraph {
        ColorGraph {
            version: 1,
            nodes: vec![
                Node {
                    id: SOURCE,
                    label: String::new(),
                    enabled: true,
                    pos: [0.0, 0.0],
                    kind: NodeKind::Source,
                },
                Node {
                    id: OUTPUT,
                    label: String::new(),
                    enabled: true,
                    pos: [480.0, 0.0],
                    kind: NodeKind::Output,
                },
            ],
            edges: Vec::new(),
            view: None,
        }
    }

    pub fn from_json(text: &str) -> Option<ColorGraph> {
        let mut g: ColorGraph = serde_json::from_str(text).ok()?;
        // Source and Output always exist
        if !g.nodes.iter().any(|n| n.id == SOURCE) || !g.nodes.iter().any(|n| n.id == OUTPUT) {
            return None;
        }
        let ids: Vec<NodeId> = g.nodes.iter().map(|n| n.id).collect();
        g.edges
            .retain(|e| ids.contains(&e.from) && ids.contains(&e.to));
        Some(g)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn node_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }

    fn next_id(&self) -> NodeId {
        self.nodes.iter().map(|n| n.id).max().unwrap_or(1) + 1
    }

    /// Adds a node (not connected) and returns its id.
    pub fn add(&mut self, kind: NodeKind, pos: [f32; 2]) -> NodeId {
        let id = self.next_id();
        self.nodes.push(Node {
            id,
            label: String::new(),
            enabled: true,
            pos,
            kind,
        });
        id
    }

    /// Edges into `id`, in order of input (pictures, then keys).
    pub fn inputs_of(&self, id: NodeId) -> Vec<Edge> {
        let mut v: Vec<Edge> = self.edges.iter().filter(|e| e.to == id).copied().collect();
        v.sort_by_key(|e| (e.key, e.input));
        v
    }

    pub fn outputs_of(&self, id: NodeId) -> Vec<Edge> {
        self.edges
            .iter()
            .filter(|e| e.from == id)
            .copied()
            .collect()
    }

    /// Whether `from` can reach `to` along the edges.
    pub fn reaches(&self, from: NodeId, to: NodeId) -> bool {
        let mut stack = vec![from];
        let mut seen = Vec::new();
        while let Some(n) = stack.pop() {
            if n == to {
                return true;
            }
            if seen.contains(&n) {
                continue;
            }
            seen.push(n);
            stack.extend(self.outputs_of(n).iter().map(|e| e.to));
        }
        false
    }

    /// Connects `from` to input `input` of `to` (replacing what was there). Refused when it
    /// would make a loop, or the nodes cannot take it.
    pub fn connect(
        &mut self,
        from: NodeId,
        to: NodeId,
        input: u8,
        key: bool,
    ) -> Result<(), &'static str> {
        let (Some(a), Some(b)) = (self.node(from), self.node(to)) else {
            return Err("no such node");
        };
        if from == to || self.reaches(to, from) {
            return Err("that would make a loop");
        }
        if matches!(b.kind, NodeKind::Source) {
            return Err("the source takes no input");
        }
        if matches!(a.kind, NodeKind::Output) {
            return Err("the output has no output");
        }
        if key && !matches!(b.kind, NodeKind::Corrector(_)) {
            return Err("only a corrector takes a key");
        }
        if !key && !b.kind.is_mixer() && input != 0 {
            return Err("that node has one input");
        }
        self.edges
            .retain(|e| !(e.to == to && e.input == input && e.key == key));
        self.edges.push(Edge {
            from,
            to,
            input,
            key,
        });
        Ok(())
    }

    pub fn disconnect(&mut self, edge: Edge) {
        self.edges.retain(|e| *e != edge);
    }

    /// Removes a node. A corrector with one picture in and its outputs is bridged: what fed it
    /// now feeds what it fed. Source and Output stay.
    pub fn remove(&mut self, id: NodeId) {
        if id == SOURCE || id == OUTPUT {
            return;
        }
        let ins: Vec<Edge> = self.inputs_of(id).into_iter().filter(|e| !e.key).collect();
        let outs = self.outputs_of(id);
        self.edges.retain(|e| e.from != id && e.to != id);
        self.nodes.retain(|n| n.id != id);
        if let [only] = ins[..] {
            for o in outs {
                let _ = self.connect(only.from, o.to, o.input, o.key);
            }
        }
        if let Some(View::Matte(v) | View::Highlight(v)) = self.view
            && v == id
        {
            self.view = None;
        }
    }

    /// Adds a corrector after `after` (between it and what it fed), like a serial node.
    pub fn add_serial(&mut self, after: NodeId) -> NodeId {
        let pos = self
            .node(after)
            .map(|n| [n.pos[0] + 150.0, n.pos[1]])
            .unwrap_or_default();
        let id = self.add(NodeKind::Corrector(Box::default()), pos);
        let outs: Vec<Edge> = self
            .outputs_of(after)
            .into_iter()
            .filter(|e| !e.key)
            .collect();
        for o in &outs {
            self.disconnect(*o);
            let _ = self.connect(id, o.to, o.input, false);
        }
        let _ = self.connect(after, id, 0, false);
        // what was after it moves right
        self.spread(id);
        id
    }

    /// Adds a corrector beside `beside`, fed by the same picture, and a parallel mixer joining
    /// the two where `beside` was going.
    pub fn add_parallel(&mut self, beside: NodeId) -> Option<NodeId> {
        let src = self
            .inputs_of(beside)
            .into_iter()
            .find(|e| !e.key && e.input == 0)?;
        let b = self.node(beside)?;
        let (bx, by) = (b.pos[0], b.pos[1]);
        let new = self.add(NodeKind::Corrector(Box::default()), [bx, by + 70.0]);
        let mixer = self.add(
            NodeKind::Parallel(Mixer::default()),
            [bx + 150.0, by + 35.0],
        );
        let outs: Vec<Edge> = self
            .outputs_of(beside)
            .into_iter()
            .filter(|e| !e.key)
            .collect();
        for o in &outs {
            self.disconnect(*o);
            let _ = self.connect(mixer, o.to, o.input, false);
        }
        let _ = self.connect(src.from, new, 0, false);
        let _ = self.connect(beside, mixer, 0, false);
        let _ = self.connect(new, mixer, 1, false);
        self.spread(mixer);
        Some(new)
    }

    /// Adds a layer mixer after `after`, with a corrector as the layer above it.
    pub fn add_layer(&mut self, after: NodeId) -> Option<NodeId> {
        let a = self.node(after)?;
        let (ax, ay) = (a.pos[0], a.pos[1]);
        let src = self
            .inputs_of(after)
            .into_iter()
            .find(|e| !e.key && e.input == 0)
            .map(|e| e.from)
            .unwrap_or(after);
        let layer = self.add(NodeKind::Corrector(Box::default()), [ax, ay + 70.0]);
        let mixer = self.add(NodeKind::Layer(Mixer::default()), [ax + 150.0, ay + 35.0]);
        let outs: Vec<Edge> = self
            .outputs_of(after)
            .into_iter()
            .filter(|e| !e.key)
            .collect();
        for o in &outs {
            self.disconnect(*o);
            let _ = self.connect(mixer, o.to, o.input, false);
        }
        let _ = self.connect(src, layer, 0, false);
        let _ = self.connect(after, mixer, 0, false);
        let _ = self.connect(layer, mixer, 1, false);
        // the layer above is cut by its own key: a window to start with
        if let Some(Node {
            kind: NodeKind::Corrector(c),
            ..
        }) = self.node_mut(layer)
        {
            c.windows.push(Window::default());
        }
        self.spread(mixer);
        Some(layer)
    }

    /// Adds a corrector after `after` that takes the inverse of its key: it grades what the
    /// node before did not select.
    pub fn add_outside(&mut self, after: NodeId) -> NodeId {
        let id = self.add_serial(after);
        let _ = self.connect(after, id, 0, true);
        if let Some(Node {
            kind: NodeKind::Corrector(c),
            ..
        }) = self.node_mut(id)
        {
            c.key_in_invert = true;
        }
        id
    }

    /// Moves everything to the right of `from` out of the way of a node just added there.
    fn spread(&mut self, from: NodeId) {
        let Some(origin) = self.node(from).map(|n| n.pos) else {
            return;
        };
        let mut moved = vec![from];
        let mut queue = self.outputs_of(from);
        while let Some(e) = queue.pop() {
            if moved.contains(&e.to) {
                continue;
            }
            moved.push(e.to);
            if let Some(n) = self.node_mut(e.to)
                && n.pos[0] <= origin[0] + 140.0
                && !matches!(n.kind, NodeKind::Source)
            {
                n.pos[0] = origin[0] + 150.0 * (moved.len() as f32).min(1.0);
            }
            queue.extend(self.outputs_of(e.to));
        }
    }

    /// The nodes that lead to Output, each after the ones it takes from.
    pub fn order(&self) -> Vec<NodeId> {
        let mut out: Vec<NodeId> = Vec::new();
        fn visit(g: &ColorGraph, id: NodeId, out: &mut Vec<NodeId>, stack: &mut Vec<NodeId>) {
            if out.contains(&id) || stack.contains(&id) {
                return;
            }
            stack.push(id);
            for e in g.inputs_of(id) {
                visit(g, e.from, out, stack);
            }
            stack.pop();
            out.push(id);
        }
        visit(self, OUTPUT, &mut out, &mut Vec::new());
        out
    }

    /// The picture goes straight through: nothing enabled changes it.
    pub fn is_identity(&self) -> bool {
        let order = self.order();
        order.iter().all(|id| match self.node(*id) {
            Some(n) => {
                !n.enabled
                    || match &n.kind {
                        NodeKind::Corrector(c) => {
                            c.is_neutral() && !c.has_matte() || c.is_neutral()
                        }
                        NodeKind::Source | NodeKind::Output => true,
                        _ => false,
                    }
            }
            None => true,
        }) && self.inputs_of(OUTPUT).iter().all(|e| {
            // everything between Source and Output is a chain of pass-through correctors
            let mut at = e.from;
            loop {
                if at == SOURCE {
                    return true;
                }
                let Some(n) = self.node(at) else { return false };
                if !matches!(n.kind, NodeKind::Corrector(_)) {
                    return false;
                }
                match self.inputs_of(at).iter().find(|i| !i.key && i.input == 0) {
                    Some(i) => at = i.from,
                    None => return false,
                }
            }
        })
    }

    /// A short label of a node for the editor: its label, else its number.
    pub fn title(&self, id: NodeId) -> String {
        match self.node(id) {
            Some(n) if !n.label.is_empty() => n.label.clone(),
            Some(n) => match n.kind {
                NodeKind::Source => "Source".into(),
                NodeKind::Output => "Output".into(),
                _ => format!("{:02}", id - 1),
            },
            None => String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_graph_runs_source_to_output_through_one_corrector() {
        let g = ColorGraph::default();
        assert_eq!(g.order(), vec![SOURCE, 2, OUTPUT]);
        assert!(g.is_identity());
        let back = ColorGraph::from_json(&g.to_json()).unwrap();
        assert_eq!(back, g);
    }

    #[test]
    fn nodes_are_added_serial_parallel_and_layered() {
        let mut g = ColorGraph::default();
        let two = g.add_serial(2);
        assert_eq!(g.order(), vec![SOURCE, 2, two, OUTPUT]);
        let three = g.add_parallel(two).unwrap();
        let order = g.order();
        // the mixer comes after both of the nodes it joins, and Output last
        let pos = |id| order.iter().position(|n| *n == id).unwrap();
        let mixer = g
            .nodes
            .iter()
            .find(|n| matches!(n.kind, NodeKind::Parallel(_)))
            .unwrap()
            .id;
        assert!(pos(two) < pos(mixer) && pos(three) < pos(mixer) && pos(mixer) < pos(OUTPUT));
        // both parallel nodes take the picture the first one took
        assert_eq!(
            g.inputs_of(three).iter().find(|e| !e.key).unwrap().from,
            g.inputs_of(two).iter().find(|e| !e.key).unwrap().from
        );
        let layer = g.add_layer(mixer).unwrap();
        assert!(g.nodes.iter().any(|n| matches!(n.kind, NodeKind::Layer(_))));
        assert!(
            matches!(&g.node(layer).unwrap().kind, NodeKind::Corrector(c) if !c.windows.is_empty())
        );
        assert_eq!(*g.order().last().unwrap(), OUTPUT);
    }

    #[test]
    fn loops_and_bad_connections_are_refused() {
        let mut g = ColorGraph::default();
        let two = g.add_serial(2);
        assert!(g.connect(two, 2, 0, false).is_err(), "a loop");
        assert!(g.connect(OUTPUT, 2, 0, false).is_err());
        assert!(g.connect(2, SOURCE, 0, false).is_err());
        assert!(g.connect(2, OUTPUT, 1, false).is_err(), "one input");
        // a key goes into a corrector only
        let m = g.add(NodeKind::Parallel(Mixer::default()), [0.0, 0.0]);
        assert!(g.connect(2, m, 0, true).is_err());
        assert!(g.connect(2, two, 0, true).is_ok());
    }

    #[test]
    fn removing_a_node_bridges_its_neighbours() {
        let mut g = ColorGraph::default();
        let two = g.add_serial(2);
        assert_eq!(g.order(), vec![SOURCE, 2, two, OUTPUT]);
        g.remove(two);
        assert_eq!(g.order(), vec![SOURCE, 2, OUTPUT]);
        // Source and Output cannot be removed
        g.remove(SOURCE);
        g.remove(OUTPUT);
        assert!(g.node(SOURCE).is_some() && g.node(OUTPUT).is_some());
        let outside = g.add_outside(2);
        assert!(g.inputs_of(outside).iter().any(|e| e.key && e.from == 2));
    }

    #[test]
    fn a_graded_node_is_not_the_identity() {
        let mut g = ColorGraph::default();
        assert!(g.is_identity());
        if let Some(Node {
            kind: NodeKind::Corrector(c),
            ..
        }) = g.node_mut(2)
        {
            c.grade.saturation = 1.4;
        }
        assert!(!g.is_identity());
        // disabled again, it passes the picture through
        g.node_mut(2).unwrap().enabled = false;
        assert!(g.is_identity());
    }
}
