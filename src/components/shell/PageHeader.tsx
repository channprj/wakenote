import type { ReactNode } from "react";

export function PageHeader({
  eyebrow,
  title,
  description,
  actions,
}: {
  eyebrow?: ReactNode;
  title: ReactNode;
  description?: ReactNode;
  actions?: ReactNode;
}) {
  return (
    <header data-slot="page-header" className="page-header">
      <div
        data-slot="page-header-copy"
        className="page-header__copy overflow-wrap-anywhere"
      >
        {eyebrow ? <span className="page-header__eyebrow">{eyebrow}</span> : null}
        <div className="page-header__title-row">
          <h1>{title}</h1>
          {description ? <p>{description}</p> : null}
        </div>
      </div>
      {actions ? (
        <div data-slot="page-header-actions" className="page-header__actions">
          {actions}
        </div>
      ) : null}
    </header>
  );
}
