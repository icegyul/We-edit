# Color Nodes

**Updated:** 2026-10-04 (version 0.9.0)

Color Nodes grades a clip with a node graph, the way colorists work: the picture goes in at a
**Source** node, passes through **correctors**, **mixers** and keys, and leaves at an **Output**
node. Open it from the Color workspace (Window > Color Nodes), or add the effect **Color Nodes**
(Effects > Color Correction) to a clip. The graph is stored in the effect's hidden `graph`
parameter as JSON, so it saves with the project and every edit is one undo step (merged while
dragging a control). Everything runs on the GPU in the same pass system as the other effects.

## Nodes

| Node | What it does |
|---|---|
| **Source** | The clip's picture. |
| **Output** | What leaves the effect. |
| **Corrector** | Grades its input: primaries, curves, LUT, blur, sharpen. Its matte (qualifier, windows, key) limits where the grade shows. |
| **Parallel Mixer** | Joins several correctors that all started from the same picture. Normal mode is a weighted average; Add, Screen, Multiply, Overlay and Difference blend the branches. |
| **Layer Mixer** | Stacks pictures: input 1 is the background, each next input is a layer cut by its own matte, with an opacity and a blend mode. |

Every corrector has two outputs: the **picture** (round, right) and the **key** (square, bottom).
A key can feed the **key input** (square, top) of another corrector, which then grades only inside
that key, or, with Invert the Key Input, **outside** it (the + Outside button does both at once).

## The editor

- **+ Serial** adds a corrector after the selected node; **+ Parallel** adds one beside it and a
  parallel mixer joining the two; **+ Layer** adds a layer above it with a layer mixer;
  **+ Outside** adds a corrector that grades what the selected node does not select.
- Drag from an output socket to an input socket to connect; right-click a connecting line to cut
  it. Loops are refused. Drag a node to move it, drag the background to pan.
- Right-click a node to add nodes, label it or delete it. Delete bridges the neighbours.
- **Disable** passes a node's picture through unchanged. **Matte** shows the selected node's
  matte in the viewer (white where selected); **Highlight** dims what it does not select.

## Corrector controls

- **Primaries**: Lift, Gamma, Gain and Offset wheels (hue and amount from the puck, a master
  value below; double-click a wheel to reset it), Contrast around a Pivot, Saturation, Hue,
  Temperature, Tint, Shadows, Highlights and Color Boost (saturates dull colors more than vivid
  ones).
- **Curves**: Master, Red, Green and Blue, plus Hue vs Hue, Hue vs Sat, Lum vs Sat and Sat vs Sat,
  where the middle of the curve means no change. Double-click to add a point, right-click to
  remove it.
- **Qualifier**: selects by hue (a range around a center that wraps the wheel), saturation and
  luminance, each with soft edges, plus blur, clean black and white and invert.
- **Windows**: rectangle, ellipse, linear gradient and polygon (3 to 8 points), each with center,
  size, rotation, softness and invert. Several windows add up.
- **Key**: gain, offset, blur and invert of the final matte.
- **LUT and effects**: a `.cube` 3D LUT with its amount, blur, sharpen and the node's amount.

## Limits

- The grade is display-referred (the sequence's working values), not scene-referred color
  management; there are no ACES or color-space nodes yet.
- Parameters are not keyframed; a graph holds one look for the whole clip.
- No tracking of windows and no node thumbnails in the editor yet.
- The formulas are this project's own and aim at the documented behavior of each control, not at
  numerical parity with any other grading system.
