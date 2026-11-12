import RustWorker from './index.js';
export { DevTestAccounts } from './index.js';
export default {fetch(request,env,ctx){return new RustWorker(ctx,env).fetch(request);}};
