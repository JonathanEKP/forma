/// <reference types="vite/client" />

/**
 * The two values the frontend knows about the identity provider, typed rather
 * than reached for through an untyped `import.meta.env`.
 *
 * Both are public: Vite bakes them into the bundle, so they reach every visitor
 * by design. Anything that must not is not a `VITE_` value.
 */
interface ImportMetaEnv {
  readonly VITE_SUPABASE_URL?: string;
  readonly VITE_SUPABASE_PUBLISHABLE_KEY?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
