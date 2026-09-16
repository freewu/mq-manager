import type { ReactNode } from 'react';

export interface PageHeaderProps {
  title: ReactNode;
  subtitle?: ReactNode;
  actions?: ReactNode;
}

/**
 * Page level heading. Most pages render this directly under the top bar so the
 * title scrolls with the content instead of eating vertical space.
 */
export function PageHeader({ title, subtitle, actions }: PageHeaderProps) {
  return (
    <div className="row-between wrap gap-12" style={{ marginBottom: 16, alignItems: 'flex-start' }}>
      <div style={{ minWidth: 0 }}>
        <h1>{title}</h1>
        {subtitle ? <div className="small muted" style={{ marginTop: 2 }}>{subtitle}</div> : null}
      </div>
      {actions ? <div className="row gap-8 wrap">{actions}</div> : null}
    </div>
  );
}
