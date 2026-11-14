# Entities and relationships

These diagrams render natively in HTML, PDF, SVG, EPUB and Flow.

## Customers and orders

```mermaid
erDiagram
    direction LR
    CUSTOMER["Customer account"] {
        int id PK
        string email UK "Sign-in address"
        string? nickname
    }
    ORDER {
        int id PK
        int customer_id FK "Account owner"
        decimal(10,2) total "Exact amount"
    }
    LINE_ITEM {
        int order_id PK, FK
        int product_id PK, FK
        int quantity
    }
    CUSTOMER ||--o{ ORDER : places
    ORDER ||--|{ LINE_ITEM : contains
    PRODUCT }o..o{ LINE_ITEM : "appears in"
```

## Optional ownership and a self-reference

```mermaid
erDiagram
    direction TB
    EMPLOYEE {
        int id PK
        int? manager_id FK
        string name
    }
    DEPARTMENT {
        int id PK
        string name UK
    }
    EMPLOYEE }o..o| DEPARTMENT : "assigned to"
    EMPLOYEE |o--o{ EMPLOYEE : manages
```
