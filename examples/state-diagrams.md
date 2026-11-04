# Native state machines

These diagrams render through the same first-party compiler in HTML, PDF, SVG,
EPUB and browser Flow.

## A request lifecycle

```mermaid
stateDiagram-v2
    [*] --> Idle
    state "Waiting for input" as Idle
    Idle --> Check: submit
    state Check <<choice>>
    Check --> Working: accepted
    Check --> Idle: retry
    Working --> Working: next item
    Working --> [*]: complete
```

## Parallel work and a join

```mermaid
stateDiagram-v2
    direction LR
    state Fork <<fork>>
    state Join <<join>>
    [*] --> Fork
    Fork --> Validate
    Fork --> Prepare
    Validate --> Join
    Prepare --> Join
    Join --> Publish
    Publish --> [*]
```

## Different events between the same states

```mermaid
stateDiagram-v2
    direction LR
    [*] --> Ready
    Ready --> Running: manual start
    Ready --> Running: scheduled start
    Running --> Ready: pause
    Running --> [*]: finish
```
