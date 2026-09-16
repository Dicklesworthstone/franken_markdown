// Production JSX consumer document (FCB-022 · fcb-9vx.8).
import React, { useState } from "react";

function StatusBadge({ status, count }) {
    return (
        <span className={`badge-${status}`} title="{literal braces in title}">
            <i className="icon-dot" />
            &bull;&nbsp;Count: {count}
        </span>
    );
}

export default function Dashboard({ items = [] }) {
    const [selected, setSelected] = useState(0);
    const hasItems = items.length > 0 && 1 < 2;

    return (
        <main className="dashboard" data-testid="main">
            <header>
                <h1>FrankenCode &trade; &mdash; Browser</h1>
                <StatusBadge status="active" count={items.length} />
            </header>
            <div className="content">
                {hasItems ? (
                    <>
                        <p>Total: {items.length} &copy; 2026</p>
                        <ul>
                            {items.map(it => (
                                <li key={it.id} onClick={() => setSelected(it.id)}>
                                    <strong>{it.name}</strong>
                                    {it.score < 50 && <span className="pill">Low</span>}
                                </li>
                            ))}
                        </ul>
                    </>
                ) : (
                    <p>No items &lt;none&gt;.</p>
                )}
            </div>
            <footer>
                <p>&copy; 2026 Authors. Licensed under Apache-2.0 &amp; MIT.</p>
            </footer>
        </main>
    );
}
