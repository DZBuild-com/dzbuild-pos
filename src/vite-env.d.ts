interface ImportMetaEnv {
  /** `1` only when the E2E suite serves the app (`playwright.config.ts`): the fake backend of
   * `src/test/e2e` replaces Rust. Unset in every real build. */
  readonly VITE_E2E?: string;
}

/** Set only when `VITE_E2E=1`: the specs move the in-memory router and emit backend events. */
interface Window {
  __dzposNavigate?: (to: string) => void;
  __dzposEmit?: (event: string, payload: unknown) => void;
}
