import type { ReactNode } from 'react';

import { Icon, type IconName } from './Icon';
import { cn } from '@/lib/cn';

export interface TabItem {
  id: string;
  label: string;
  icon?: IconName;
  count?: number;
  disabled?: boolean;
}

export function Tabs({
  tabs,
  active,
  onChange,
  actions,
}: {
  tabs: TabItem[];
  active: string;
  onChange: (id: string) => void;
  actions?: ReactNode;
}) {
  return (
    <div className="tabs">
      {tabs.map((tab) => (
        <button
          key={tab.id}
          type="button"
          disabled={tab.disabled}
          className={cn('tab', tab.id === active && 'tab--active')}
          style={tab.disabled ? { opacity: 0.4, cursor: 'not-allowed' } : undefined}
          onClick={() => onChange(tab.id)}
        >
          <span className="row gap-6" style={{ display: 'inline-flex' }}>
            {tab.icon ? <Icon name={tab.icon} size={13} /> : null}
            {tab.label}
            {tab.count !== undefined ? <span className="tab__count">{tab.count}</span> : null}
          </span>
        </button>
      ))}
      {actions ? (
        <div className="row gap-6" style={{ marginLeft: 'auto', paddingRight: 6 }}>
          {actions}
        </div>
      ) : null}
    </div>
  );
}
