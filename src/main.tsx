import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';
import { initI18n } from './i18n';
import './styles.css';

async function bootstrap() {
  await initI18n();
  const rootEl = document.getElementById('root');
  if (!rootEl) throw new Error('Root element #root not found');
  ReactDOM.createRoot(rootEl).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>
  );
}

bootstrap().catch((e) => {
  console.error('[bootstrap] failed', e);
});
