// Test-only Owner-shaped caller of the actual named production entrypoint.
export default {
  async fetch(request, env) {
    const raw = await request.text();
    const path = new URL(request.url).pathname;
    if (path === '/default-approve' || path === '/default-fence') {
      const method = path === '/default-approve' ? 'owner_approve_provision' : 'read_recovery_fence';
      try {
        await (path === '/default-approve' ? env.OWNER_DEFAULT : env.FENCE_DEFAULT)[method](raw);
        return Response.json({missing_rpc:false});
      } catch(error) {
        const message = String(error);
        return Response.json({missing_rpc:message.includes(method) && /does not implement|not a function|not implemented/i.test(message)},{status:409});
      }
    }
    if (new URL(request.url).pathname === '/pump') {
      try { await env.PUMP.tick(); return new Response(null, { status: 204 }); }
      catch { return new Response('scheduled delivery rejected', { status: 503 }); }
    }
    if (new URL(request.url).pathname === '/default-verify') {
      try {
        await env.IDENTITY_DEFAULT.native_verify_approval(raw);
        return Response.json({ missing_rpc: false });
      } catch (error) {
        const message = String(error);
        const missing = message.includes('native_verify_approval') && /does not implement|not a function|not implemented/i.test(message);
        return Response.json({ missing_rpc: missing }, { status: 409 });
      }
    }
    try {
      const body = await env.IDENTITY.verifyApproval(raw);
      return new Response(body, { headers: { 'Content-Type': 'application/json' } });
    } catch {
      return Response.json({ rejected: true }, { status: 409 });
    }
  },
};
