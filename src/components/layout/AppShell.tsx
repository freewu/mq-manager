import { useEffect, useState, type ReactNode } from 'react';

import { StatusBar } from './StatusBar';
import { Sidebar } from './Sidebar';
import { TopBar } from './TopBar';
import { ToastHost } from '@/components/ui/ToastHost';
import { cn } from '@/lib/cn';

export function AppShell({
  version,
  onNewConnection,
  children,
}: {
  version: string;
  onNewConnection: () => void;
  children: ReactNode;
}) {
  const [collapsed, setCollapsed] = useState(false);

  // Ctrl/Cmd+B toggles the sidebar, matching every editor the user already knows.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'b') {
        event.preventDefault();
        setCollapsed((value) => !value);
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, []);

  return (
    <div className={cn('app', collapsed && 'app--collapsed')}>
      <Sidebar version={version} onNewConnection={onNewConnection} />
      <div className="main">
        <TopBar onToggleSidebar={() => setCollapsed((value) => !value)} />
        {children}
        <StatusBar version={version} />
      </div>
      <ToastHost />
    </div>
  );
}
