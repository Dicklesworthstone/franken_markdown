# Sequence diagrams with complete interaction semantics

The same bounded first-party compiler produces native vector figures in HTML,
PDF, SVG posters, and EPUB. These examples require no Mermaid.js or network.

## Requests, retries, and alternatives

```mermaid
sequenceDiagram
    title Retried request
    accTitle: Request handling with retry and overload alternatives
    accDescr: A customer submits a request to an application. The application retries internally until accepted, reports either acceptance or overload, and completes the request.
    autonumber
    actor U as Customer
    participant A as Application
    U->>+A: Submit request
    loop Until accepted
        A->>A: Retry internally
        alt accepted
            A-->>U: Accepted
        else overloaded
            A-->>U: Retry later
        end
    end
    A-->>-U: Complete
    Note left of U: Keep the original<br/>request identifier
```

## Parallel work and conditional recovery

```mermaid
sequenceDiagram
    title Load a document
    participant Editor
    participant Parser
    participant Assets as Local assets
    par Parse document
        Editor->>Parser: Markdown source
        Parser-->>Editor: Document tree
    and Resolve images
        critical Read local resources
            Editor->>Assets: Requested bindings
            Assets-->>Editor: Image bytes
        option Missing image
            Editor->>Editor: Retain the source reference
        end
    end
    opt User exports
        Editor-)Parser: Generate publication
    end
    Note over Editor,Assets: Source and attachments remain local
```
