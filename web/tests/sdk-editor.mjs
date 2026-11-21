// The editor's classes for the browser harnesses: a page imports this file through Vite, which
// resolves @kition/sdk/editor exactly as it does for the editor itself, so KitEngine here is the
// class the running editor uses, and patching its prototype observes the live engine.
export { KitEngine } from '@kition/sdk/editor'
