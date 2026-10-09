/** What the app exposes to the specs when it runs with `VITE_E2E=1` (`src/test/e2e/install.ts`
 * and the router's E2E navigator); absent from a production build. */
interface Window {
  __dzposNavigate?: (to: string) => void;
  __dzposEmit?: (event: string, payload: unknown) => void;
}
