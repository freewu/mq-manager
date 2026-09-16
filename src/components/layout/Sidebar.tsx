import { NavLink, useLocation, useNavigate } from 'react-router-dom';

import { Icon, type IconName } from '@/components/ui/Icon';
import { StatusDot, IconButton } from '@/components/ui/primitives';
import { useCapabilities } from '@/hooks/useConnection';
import { cn } from '@/lib/cn';
import { useWorkspace } from '@/store/workspace';

function BrandMark() {
  return (
    <span className="brand__mark">
      <svg viewBox="0 0 24 24" fill="none" stroke="#fff" strokeWidth={2} strokeLinecap="round">
        <path d="M4 7h16" />
        <path d="M4 12h11" />
        <path d="M4 17h7" />
        <circle cx="19" cy="17" r="2" fill="#fff" stroke="none" />
      </svg>
    </span>
  );
}

interface NavEntry {
  to: string;
  label: string;
  icon: IconName;
  end?: boolean;
  visible: boolean;
}

export function Sidebar({
  version,
  onNewConnection,
}: {
  version: string;
  onNewConnection: () => void;
}) {
  const profiles = useWorkspace((state) => state.profiles);
  const providers = useWorkspace((state) => state.providers);
  const statuses = useWorkspace((state) => state.statuses);
  const navigate = useNavigate();
  const location = useLocation();

  const match = /^\/c\/([^/]+)/.exec(location.pathname);
  const activeId = match?.[1];
  const capabilities = useCapabilities(activeId);

  const nav: NavEntry[] = activeId
    ? [
        { to: `/c/${activeId}`, label: 'Overview', icon: 'activity', end: true, visible: true },
        {
          to: `/c/${activeId}/topics`,
          label: 'Topics',
          icon: 'layers',
          visible: capabilities.topics.list,
        },
        {
          to: `/c/${activeId}/groups`,
          label: 'Consumer groups',
          icon: 'users',
          visible: capabilities.groups.list,
        },
        {
          to: `/c/${activeId}/nodes`,
          label: 'Nodes',
          icon: 'server',
          visible: capabilities.nodes,
        },
      ]
    : [];

  return (
    <aside className="sidebar">
      <div className="sidebar__brand">
        <BrandMark />
        <div className="grow" style={{ minWidth: 0 }}>
          <div className="brand__name">MQ Manager</div>
          <div className="brand__version">v{version}</div>
        </div>
      </div>

      <div className="sidebar__scroll">
        <div className="sidebar__section">
          <span>Connections</span>
          <IconButton icon="plus" label="New connection" size="sm" onClick={onNewConnection} />
        </div>

        {profiles.length === 0 ? (
          <div className="sidebar__empty">
            No connections yet. Add a broker to get started.
          </div>
        ) : null}

        {profiles.map((profile) => {
          const state = statuses[profile.id]?.state ?? 'disconnected';
          const provider = providers.find((entry) => entry.id === profile.provider);
          return (
            <button
              key={profile.id}
              type="button"
              className={cn('nav-item', profile.id === activeId && 'nav-item--active')}
              onClick={() => navigate(`/c/${profile.id}`)}
            >
              <span className="nav-item__icon">
                <StatusDot state={state} />
              </span>
              <span className="nav-item__label" title={profile.name}>
                {profile.name}
              </span>
              <span className="tiny dim nowrap">{provider?.name ?? profile.provider}</span>
            </button>
          );
        })}

        {activeId && nav.length > 0 ? (
          <>
            <div className="sidebar__section">
              <span>Workspace</span>
            </div>
            {nav
              .filter((entry) => entry.visible)
              .map((entry) => (
                <NavLink
                  key={entry.to}
                  to={entry.to}
                  end={entry.end}
                  className={({ isActive }) => cn('nav-item', isActive && 'nav-item--active')}
                >
                  <span className="nav-item__icon">
                    <Icon name={entry.icon} size={15} />
                  </span>
                  <span className="nav-item__label">{entry.label}</span>
                </NavLink>
              ))}
          </>
        ) : null}

        <div className="sidebar__section">
          <span>Application</span>
        </div>
        <NavLink
          to="/connections"
          className={({ isActive }) => cn('nav-item', isActive && 'nav-item--active')}
        >
          <span className="nav-item__icon">
            <Icon name="plug" size={15} />
          </span>
          <span className="nav-item__label">Manage connections</span>
        </NavLink>
        <NavLink
          to="/settings"
          className={({ isActive }) => cn('nav-item', isActive && 'nav-item--active')}
        >
          <span className="nav-item__icon">
            <Icon name="settings" size={15} />
          </span>
          <span className="nav-item__label">Settings</span>
        </NavLink>
      </div>

      <div className="sidebar__foot">
        <NavLink to="/about" className={({ isActive }) => cn('nav-item', isActive && 'nav-item--active')}>
          <span className="nav-item__icon">
            <Icon name="info" size={15} />
          </span>
          <span className="nav-item__label">About</span>
        </NavLink>
      </div>
    </aside>
  );
}
