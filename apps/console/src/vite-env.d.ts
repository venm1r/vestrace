/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_VESTRACE_WORKSPACE_ID?: string;
  readonly VITE_VESTRACE_PRINCIPAL_ID?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
