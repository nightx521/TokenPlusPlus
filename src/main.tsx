import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';
import AgentDetails from './AgentDetails';
import './style.css';
import './desktop.css';
if ('__TAURI_INTERNALS__' in window) document.documentElement.classList.add('native');
ReactDOM.createRoot(document.getElementById('root')!).render(<React.StrictMode>{new URLSearchParams(location.search).get('view')==='agent-details'?<AgentDetails/>:<App/>}</React.StrictMode>);
