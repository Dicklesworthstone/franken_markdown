// FCB-009/FCB-071 consumer verification document (TSX).
// Exercises: function components, typed props, generic hooks, fragments,
// embedded expression containers, self-closing elements, entities.
import React, { useState, useEffect } from "react";

interface Props {
  title: string;
  count?: number;
  onSelect: (id: string) => void;
}

export const ConsumerPanel: React.FC<Props> = ({ title, count = 0, onSelect }) => {
  const [items, setItems] = useState<string[]>([]);
  useEffect(() => {
    setItems(prev => [...prev, title]);
  }, [title]);

  const doubled = count * 2;
  return (
    <>
      <section className="panel">
        <h2>{title}</h2>
        <p data-testid="doubled">{doubled}</p>
        <ul>
          {items.map((item, index) => (
            <li key={index} onClick={() => onSelect(item)}>
              {item} &mdash; #{index + 1}
            </li>
          ))}
        </ul>
        <br />
        <img src="/logo.png" alt={"logo " + title} />
      </section>
    </>
  );
};
