-- Forma's Custom Access Token Hook (plan section 111.4).
--
-- What it is for: the audience is the only thing separating a browser session
-- from an agent credential. Without it both look identical, and `/mcp` would
-- accept a token that exists in somebody's browser tab.
--
-- Applied by hand in the Supabase SQL Editor, then enabled at
-- Authentication -> Hooks -> Customize Access Token. It lives here because the
-- alternative is a function that exists only inside a dashboard, where nobody
-- can read what it does or notice when it changes. Supabase's GitHub
-- integration is deliberately not connected (§106.2), so this file is the
-- record.

create or replace function public.forma_access_token_hook(event jsonb)
returns jsonb
language plpgsql
stable
as $$
declare
  claims    jsonb := event -> 'claims';
  client_id text;
begin
  -- Read both places it might live. The documented examples are the HTTP
  -- variant, where `client_id` is a top-level field; the Postgres event's shape
  -- is not stated as clearly. Guessing produces a hook that silently does
  -- nothing, and then a verifier widened to accept whatever arrives — which is
  -- a verifier that checks nothing.
  client_id := coalesce(event ->> 'client_id', claims ->> 'client_id');

  -- A direct user session keeps the audience Supabase already issues. Only an
  -- OAuth client's token is re-aimed at Forma's MCP resource, and the client is
  -- named in it so one grant's tokens cannot be spent as another's.
  if client_id is not null then
    claims := jsonb_set(
      claims,
      '{aud}',
      to_jsonb('https://forma.hintly.online/mcp/' || client_id)
    );
  end if;

  return jsonb_build_object('claims', claims);
end;
$$;

-- Auth runs the hook as `supabase_auth_admin`, which needs to reach the schema
-- as well as the function. Granting only the function leaves it unable to call
-- it, and the failure looks like the hook simply not running.
grant usage on schema public to supabase_auth_admin;
grant execute on function public.forma_access_token_hook to supabase_auth_admin;

-- And nobody else. Supabase recommends against `security definer` here because
-- it would hand the function the `postgres` role's reach; an explicit grant to
-- the one caller that needs it is the narrower answer.
revoke execute on function public.forma_access_token_hook from authenticated, anon, public;
