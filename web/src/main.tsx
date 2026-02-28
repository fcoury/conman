import ReactDOM from 'react-dom/client';
import { BrowserRouter } from 'react-router-dom';

import ClientProvider from './app/components/client-provider';
import App from './App';

import './index.css';

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <BrowserRouter>
    <ClientProvider>
      <App />
    </ClientProvider>
  </BrowserRouter>,
);
