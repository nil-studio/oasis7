// Test-only Owner-shaped caller of the actual named production entrypoint.
export default {
  async fetch(request, env) {
    const raw = await request.text();
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
