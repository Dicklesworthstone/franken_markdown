# Native flowcharts

The same first-party diagram compiler serves HTML, PDF, SVG, and EPUB output.

## A chain with a decision

```mermaid
flowchart LR
    A([Receive]) --> B[Validate] --> C{Publish?}
    C -->|Yes| D([Complete])
    C -->|No| E[Review]
```

## Several dependencies on one line

```mermaid
graph TD; A[Design] & B[Research] --> C[Implement] & D[Document] --> E[Release];
```

## Quoted text and a right-to-left direction

```mermaid
flowchart RL
    A["Read A --> B; this is one label"] -- "accept | reject" --> B((Review))
    B -. retry .-> C[Check]
    B == approved ==> D([Publish])
    C --- D
```

## Bottom-to-top dependencies

```mermaid
flowchart BT
    A[Raw input] --> B[Normalize] --> C[Present]
    A --> C
```
