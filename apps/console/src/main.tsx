import React from 'react';
import ReactDOM from 'react-dom/client';
import { BrowserRouter, Routes, Route } from 'react-router-dom';
import './design-system/tokens/theme.css';
import { AppLayout } from './shell/AppLayout';
import { HomePage } from './routes/HomePage';
import { RunsPage } from './routes/RunsPage';
import { ArtifactsPage } from './routes/ArtifactsPage';
import { AgentsPage } from './routes/AgentsPage';
import { WorkflowsPage } from './routes/WorkflowsPage';
import { TriggersPage } from './routes/TriggersPage';
import { ConnectionsPage } from './routes/ConnectionsPage';
import { ModelsPage } from './routes/ModelsPage';
import { EvaluationsPage } from './routes/EvaluationsPage';
import { AuditPage } from './routes/AuditPage';
import { SettingsPage } from './routes/SettingsPage';
import { ProfilePage } from './routes/ProfilePage';

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <BrowserRouter>
      <AppLayout>
        <Routes>
          <Route path="/" element={<HomePage />} />
          <Route path="/runs" element={<RunsPage />} />
          <Route path="/artifacts" element={<ArtifactsPage />} />
          <Route path="/agents" element={<AgentsPage />} />
          <Route path="/workflows" element={<WorkflowsPage />} />
          <Route path="/triggers" element={<TriggersPage />} />
          <Route path="/connections" element={<ConnectionsPage />} />
          <Route path="/models" element={<ModelsPage />} />
          <Route path="/evaluations" element={<EvaluationsPage />} />
          <Route path="/audit" element={<AuditPage />} />
          <Route path="/settings" element={<SettingsPage />} />
          <Route path="/profile" element={<ProfilePage />} />
        </Routes>
      </AppLayout>
    </BrowserRouter>
  </React.StrictMode>
);
