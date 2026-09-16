import { useEffect, useState } from 'react';
import { HashRouter, Navigate, Route, Routes, useNavigate } from 'react-router-dom';

import { AppShell } from '@/components/layout/AppShell';
import { Banner, Button, Spinner } from '@/components/ui/primitives';
import { About } from '@/pages/About';
import { Connections } from '@/pages/Connections';
import { GroupDetail } from '@/pages/GroupDetail';
import { Groups } from '@/pages/Groups';
import { Nodes } from '@/pages/Nodes';
import { Overview } from '@/pages/Overview';
import { Settings } from '@/pages/Settings';
import { TopicDetail } from '@/pages/TopicDetail';
import { Topics } from '@/pages/Topics';
import { Welcome } from '@/pages/Welcome';
import { errorMessage } from '@/api/client';
import { useAppInfo } from '@/store/app';
import { useSettings } from '@/store/settings';
import { useWorkspace } from '@/store/workspace';

function BootScreen({ error }: { error: string | null }) {
  return (
    <div className="center" style={{ height: '100vh', flexDirection: 'column', gap: 16, padding: 24 }}>
      <span className="brand__mark" style={{ width: 44, height: 44, borderRadius: 12 }}>
        <svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="#fff" strokeWidth={2} strokeLinecap="round">
          <path d="M4 7h16" />
          <path d="M4 12h11" />
          <path d="M4 17h7" />
          <circle cx="19" cy="17" r="2" fill="#fff" stroke="none" />
        </svg>
      </span>

      {error ? (
        <div style={{ width: 'min(560px, 100%)' }}>
          <Banner
            tone="danger"
            actions={
              <Button size="sm" icon="refresh" onClick={() => window.location.reload()}>
                Retry
              </Button>
            }
          >
            <strong>Could not start the workspace.</strong>
            <div className="mt-8 mono small">{error}</div>
            <div className="mt-8 small">
              If you opened the dev server in a browser, launch the desktop shell instead:
              <span className="mono"> just dev</span>.
            </div>
          </Banner>
        </div>
      ) : (
        <div className="row gap-8 dim small">
          <Spinner />
          Loading workspace…
        </div>
      )}
    </div>
  );
}

function NotFound() {
  return (
    <div className="page">
      <div className="empty">
        <span className="empty__title">Page not found</span>
        <span className="empty__text">The view you asked for does not exist.</span>
      </div>
    </div>
  );
}

function Shell({ version }: { version: string }) {
  const navigate = useNavigate();

  return (
    <AppShell version={version} onNewConnection={() => navigate('/connections?new=1')}>
      <Routes>
        <Route path="/" element={<Welcome />} />
        <Route path="/connections" element={<Connections />} />
        <Route path="/settings" element={<Settings />} />
        <Route path="/about" element={<About />} />

        <Route path="/c/:id" element={<Overview />} />
        <Route path="/c/:id/topics" element={<Topics />} />
        <Route path="/c/:id/topics/:topic" element={<TopicDetail />} />
        <Route path="/c/:id/groups" element={<Groups />} />
        <Route path="/c/:id/groups/:group" element={<GroupDetail />} />
        <Route path="/c/:id/nodes" element={<Nodes />} />

        {/* Legacy / typo friendly redirect. */}
        <Route path="/c/:id/*" element={<Navigate to="." replace />} />
        <Route path="*" element={<NotFound />} />
      </Routes>
    </AppShell>
  );
}

export function App() {
  const init = useWorkspace((state) => state.init);
  const ready = useWorkspace((state) => state.ready);
  const loadSettings = useSettings((state) => state.load);
  const loadInfo = useAppInfo((state) => state.load);
  const version = useAppInfo((state) => state.info?.version ?? '0.1.0');

  const [error, setError] = useState<string | null>(null);
  const [booting, setBooting] = useState(true);

  useEffect(() => {
    let cancelled = false;

    void (async () => {
      try {
        await loadInfo();
        const settings = await loadSettings();
        await init();

        if (settings.autoConnect && !cancelled) {
          const workspace = useWorkspace.getState();
          for (const profile of workspace.profiles) {
            void workspace.connect(profile.id);
          }
        }
      } catch (caught) {
        if (!cancelled) setError(errorMessage(caught));
      } finally {
        if (!cancelled) setBooting(false);
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [init, loadSettings, loadInfo]);

  if (booting || !ready) return <BootScreen error={error} />;

  return (
    <HashRouter>
      <Shell version={version} />
    </HashRouter>
  );
}
