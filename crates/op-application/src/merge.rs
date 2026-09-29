//! Merging one project into another (File > Import of a project): every identity is
//! reassigned so nothing collides, and the imported content goes into its own bin.

use std::collections::HashMap;
use std::sync::Arc;

use op_core::*;

struct Remap<'a> {
    ids: &'a mut IdGen,
    items: HashMap<ItemId, ItemId>,
    assets: HashMap<AssetId, AssetId>,
    seqs: HashMap<SequenceId, SequenceId>,
    links: HashMap<LinkId, LinkId>,
    groups: HashMap<GroupId, GroupId>,
    clips: HashMap<ClipId, ClipId>,
}

impl Remap<'_> {
    fn source(&self, s: &ClipSource) -> ClipSource {
        match s {
            ClipSource::Asset {
                asset,
                item,
                stream,
            } => ClipSource::Asset {
                asset: self.assets.get(asset).copied().unwrap_or(*asset),
                item: item.and_then(|i| self.items.get(&i).copied()),
                stream: *stream,
            },
            ClipSource::Sequence { sequence, item } => ClipSource::Sequence {
                sequence: self.seqs.get(sequence).copied().unwrap_or(*sequence),
                item: item.and_then(|i| self.items.get(&i).copied()),
            },
            ClipSource::Generator { item } => ClipSource::Generator {
                item: self.items.get(item).copied().unwrap_or(*item),
            },
            ClipSource::Graphic => ClipSource::Graphic,
        }
    }

    fn components(&mut self, v: &[Component]) -> Vec<Component> {
        v.iter()
            .map(|c| {
                let mut c = c.clone();
                c.id = self.ids.component();
                c
            })
            .collect()
    }

    fn markers<D>(&mut self, v: &[Marker<D>]) -> Vec<Marker<D>> {
        v.iter()
            .map(|m| {
                let mut m = m.clone();
                m.id = self.ids.marker();
                m
            })
            .collect()
    }

    fn track(&mut self, t: &Track) -> Track {
        let mut out = t.clone();
        out.id = self.ids.track();
        out.components = self.components(&t.components);
        out.clips = t
            .clips
            .iter()
            .map(|c| {
                let mut n = c.clone();
                n.id = self.ids.clip();
                self.clips.insert(c.id, n.id);
                n.source = self.source(&c.source);
                n.components = self.components(&c.components);
                n.link = c
                    .link
                    .map(|l| *self.links.entry(l).or_insert_with(|| self.ids.link()));
                n.group = c
                    .group
                    .map(|g| *self.groups.entry(g).or_insert_with(|| self.ids.group()));
                n
            })
            .collect();
        out.transitions = t
            .transitions
            .iter()
            .map(|tr| {
                let mut n = tr.clone();
                n.id = self.ids.transition();
                n.from = tr.from.and_then(|c| self.clips.get(&c).copied());
                n.to = tr.to.and_then(|c| self.clips.get(&c).copied());
                n
            })
            .collect();
        out
    }
}

/// Adds everything in `src` to `dst` inside a new bin called `name` under `bin`.
pub fn merge_project(dst: &mut Project, src: Project, bin: ItemId, name: &str) {
    let target = dst.add_bin(bin, name);
    let mut ids = dst.ids.clone();
    let mut r = Remap {
        ids: &mut ids,
        items: HashMap::new(),
        assets: HashMap::new(),
        seqs: HashMap::new(),
        links: HashMap::new(),
        groups: HashMap::new(),
        clips: HashMap::new(),
    };
    r.items.insert(src.root, target);
    for id in src.items.keys() {
        if *id != src.root {
            let n = r.ids.item();
            r.items.insert(*id, n);
        }
    }
    for id in src.assets.keys() {
        let n = r.ids.asset();
        r.assets.insert(*id, n);
    }
    for id in src.sequences.keys() {
        let n = r.ids.sequence();
        r.seqs.insert(*id, n);
    }
    for (old, a) in &src.assets {
        let mut a = (**a).clone();
        a.id = r.assets[old];
        dst.assets.insert(a.id, Arc::new(a));
    }
    for (old, s) in &src.sequences {
        let mut n = Sequence {
            id: r.seqs[old],
            name: s.name.clone(),
            settings: s.settings.clone(),
            video: Vec::new(),
            audio: Vec::new(),
            markers: Vec::new(),
            mark_in: s.mark_in,
            mark_out: s.mark_out,
            playhead: s.playhead,
            master: Vec::new(),
        };
        n.video = s.video.iter().map(|t| Arc::new(r.track(t))).collect();
        n.audio = s.audio.iter().map(|t| Arc::new(r.track(t))).collect();
        n.markers = r.markers(&s.markers);
        n.master = r.components(&s.master);
        dst.sequences.insert(n.id, Arc::new(n));
    }
    // items, parents before children (walk order)
    for (_, old) in src.walk() {
        let Some(it) = src.item(old) else { continue };
        let mut n = it.clone();
        n.id = r.items[&old];
        n.parent = it.parent.and_then(|p| r.items.get(&p).copied());
        n.markers = r.markers(&it.markers);
        n.kind = match &it.kind {
            ItemKind::Bin { children } => ItemKind::Bin {
                children: children
                    .iter()
                    .filter_map(|c| r.items.get(c).copied())
                    .collect(),
            },
            ItemKind::Media { asset, subclip } => ItemKind::Media {
                asset: r.assets.get(asset).copied().unwrap_or(*asset),
                subclip: *subclip,
            },
            ItemKind::Sequence { sequence } => ItemKind::Sequence {
                sequence: r.seqs.get(sequence).copied().unwrap_or(*sequence),
            },
            other => other.clone(),
        };
        dst.items.insert(n.id, Arc::new(n));
    }
    let top: Vec<ItemId> = src
        .children(src.root)
        .iter()
        .filter_map(|c| r.items.get(c).copied())
        .collect();
    if let Some(ItemKind::Bin { children }) = dst.item_mut(target).map(|i| &mut i.kind) {
        children.extend(top);
    }
    dst.ids = ids;
    dst.foreign.extend(src.foreign);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merged_ids_do_not_collide() {
        let mut a = Project::new("a");
        a.add_sequence(a.root, "A", SequenceSettings::default());
        let mut b = Project::new("b");
        let bin = b.add_bin(b.root, "Inner");
        b.add_sequence(bin, "B", SequenceSettings::default());
        let root = a.root;
        merge_project(&mut a, b, root, "Imported");
        a.validate().unwrap();
        assert_eq!(a.sequences.len(), 2);
        // A, Imported, Imported/Inner, Imported/Inner/B
        assert_eq!(a.walk().len(), 4);
        let mut all = std::collections::HashSet::new();
        for id in a.items.keys() {
            assert!(all.insert(id.0));
        }
    }
}
