// The Kit UI protocol's peer for the browser harnesses: the development host page imports this
// file through Vite, which resolves @kition/sdk/protocol to the module its host side already runs,
// so KitPeer here is the class of the host's live peer.
export { KitPeer } from '@kition/sdk/protocol'
