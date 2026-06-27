# Graph Report - /home/igris/cosmic-connect  (2026-06-27)

## Corpus Check
- cluster-only mode — file stats not available

## Summary
- 74 nodes · 180 edges · 10 communities (9 shown, 1 thin omitted)
- Extraction: 100% EXTRACTED · 0% INFERRED · 0% AMBIGUOUS
- Token cost: 0 input · 0 output

## Community Hubs (Navigation)
- [[_COMMUNITY_Community 0|Community 0]]
- [[_COMMUNITY_Community 1|Community 1]]
- [[_COMMUNITY_Community 2|Community 2]]
- [[_COMMUNITY_Community 3|Community 3]]
- [[_COMMUNITY_Community 4|Community 4]]
- [[_COMMUNITY_Community 5|Community 5]]
- [[_COMMUNITY_Community 6|Community 6]]

## God Nodes (most connected - your core abstractions)
1. `KdeConnectBackend` - 29 edges
2. `CosmicConnect` - 20 edges
3. `Device` - 12 edges
4. `Message` - 9 edges
5. `device_row()` - 6 edges
6. `PollState` - 5 edges
7. `DeviceType` - 5 edges
8. `main()` - 2 edges
9. `BatteryInfo` - 2 edges
10. `ActionType` - 2 edges

## Surprising Connections (you probably didn't know these)
- `CosmicConnect` --references--> `KdeConnectBackend`  [EXTRACTED]
  src/app.rs → src/backend/mod.rs
- `CosmicConnect` --references--> `Device`  [EXTRACTED]
  src/app.rs → src/model.rs
- `PollState` --references--> `KdeConnectBackend`  [EXTRACTED]
  src/app.rs → src/backend/mod.rs
- `device_row()` --references--> `Device`  [EXTRACTED]
  src/app.rs → src/model.rs

## Import Cycles
- None detected.

## Communities (10 total, 1 thin omitted)

### Community 1 - "Community 1"
Cohesion: 0.40
Nodes (3): Result, main(), String

### Community 2 - "Community 2"
Cohesion: 0.23
Nodes (6): ActionType, BatteryInfo, Device, DeviceEvent, DeviceType, Vec

### Community 3 - "Community 3"
Cohesion: 0.36
Nodes (6): Application, Arc, Core, Id, Option, CosmicConnect

### Community 4 - "Community 4"
Cohesion: 0.52
Nodes (3): Element, device_row(), Message

### Community 5 - "Community 5"
Cohesion: 0.38
Nodes (3): Self, PollState, Subscription

### Community 6 - "Community 6"
Cohesion: 0.50
Nodes (3): Action, Flags, Task

## Knowledge Gaps
- **1 isolated node(s):** `DeviceEvent`
  These have ≤1 connection - possible missing edges or undocumented components.
- **1 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `KdeConnectBackend` connect `Community 0` to `Community 1`, `Community 2`, `Community 3`, `Community 5`?**
  _High betweenness centrality (0.349) - this node is a cross-community bridge._
- **Why does `CosmicConnect` connect `Community 3` to `Community 0`, `Community 1`, `Community 2`, `Community 4`, `Community 5`, `Community 6`?**
  _High betweenness centrality (0.329) - this node is a cross-community bridge._
- **Why does `Device` connect `Community 2` to `Community 1`, `Community 3`, `Community 4`, `Community 5`?**
  _High betweenness centrality (0.179) - this node is a cross-community bridge._
- **What connects `DeviceEvent` to the rest of the system?**
  _1 weakly-connected nodes found - possible documentation gaps or missing edges._