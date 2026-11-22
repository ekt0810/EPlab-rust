CREATE TABLE IF NOT EXISTS content_seeds (version integer PRIMARY KEY, created_at timestamptz NOT NULL DEFAULT now());
CREATE TABLE IF NOT EXISTS users (
 id uuid PRIMARY KEY DEFAULT gen_random_uuid(), email text NOT NULL UNIQUE CHECK(email=lower(email)),
 name text NOT NULL, phone text NOT NULL DEFAULT '', password_hash text NOT NULL,
 role text NOT NULL DEFAULT 'student' CHECK(role IN('admin','student')),
 admin_slot smallint UNIQUE CHECK(admin_slot IN(1,2)),
 verified boolean NOT NULL DEFAULT false, blocked boolean NOT NULL DEFAULT false,
 setup_pending boolean NOT NULL DEFAULT false, created_at timestamptz NOT NULL DEFAULT now(),
 last_login_at timestamptz, CHECK((role='admin')=(admin_slot IS NOT NULL))
);
CREATE TABLE IF NOT EXISTS sessions (
 id text PRIMARY KEY, user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
 csrf text NOT NULL, user_agent text NOT NULL DEFAULT '', created_at timestamptz NOT NULL DEFAULT now(),
 expires_at timestamptz NOT NULL, reauth_at timestamptz
);
CREATE INDEX IF NOT EXISTS sessions_user ON sessions(user_id);
CREATE TABLE IF NOT EXISTS tokens (
 hash text PRIMARY KEY, user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
 purpose text NOT NULL CHECK(purpose IN('verify','reset','invite')), expires_at timestamptz NOT NULL
);
CREATE TABLE IF NOT EXISTS rate_limits (
 key text PRIMARY KEY, attempts integer NOT NULL DEFAULT 1, window_start timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE IF NOT EXISTS lab_memberships (
 user_id uuid PRIMARY KEY REFERENCES users(id), status text NOT NULL CHECK(status IN('pending','approved','rejected','revoked')),
 tracks text[] NOT NULL, message text NOT NULL, decision_note text NOT NULL DEFAULT '',
 reviewed_by uuid REFERENCES users(id), revision integer NOT NULL DEFAULT 1,
 created_at timestamptz NOT NULL DEFAULT now(), updated_at timestamptz NOT NULL DEFAULT now(), reviewed_at timestamptz
);
CREATE INDEX IF NOT EXISTS membership_status ON lab_memberships(status,updated_at DESC);
CREATE TABLE IF NOT EXISTS audit_logs (
 id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, actor_id uuid REFERENCES users(id),
 action text NOT NULL, target text NOT NULL DEFAULT '', metadata jsonb NOT NULL DEFAULT '{}',
 created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS audit_target ON audit_logs(target,id DESC);
CREATE TABLE IF NOT EXISTS courses (
 id uuid PRIMARY KEY DEFAULT gen_random_uuid(), category text UNIQUE NOT NULL CHECK(category IN('embedded','kicad','solidworks')),
 title text NOT NULL, description text NOT NULL, status text NOT NULL DEFAULT 'published' CHECK(status IN('draft','published')),
 position integer NOT NULL DEFAULT 0, revision integer NOT NULL DEFAULT 1
);
CREATE TABLE IF NOT EXISTS lessons (
 track text NOT NULL REFERENCES courses(category), id text NOT NULL,
 body jsonb NOT NULL, status text NOT NULL DEFAULT 'published' CHECK(status IN('draft','published')),
 revision integer NOT NULL DEFAULT 1, position integer NOT NULL,
 PRIMARY KEY(track,id)
);
CREATE TABLE IF NOT EXISTS classroom_progress (
 user_id uuid NOT NULL REFERENCES users(id), track text NOT NULL, lesson_id text NOT NULL,
 scene integer NOT NULL DEFAULT 0, note text NOT NULL DEFAULT '', checklist jsonb NOT NULL DEFAULT '[]',
 quiz_passed boolean NOT NULL DEFAULT false, completed boolean NOT NULL DEFAULT false,
 updated_at timestamptz NOT NULL DEFAULT now(), PRIMARY KEY(user_id,track,lesson_id),
 FOREIGN KEY(track,lesson_id) REFERENCES lessons(track,id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS classroom_questions (
 id uuid PRIMARY KEY DEFAULT gen_random_uuid(), user_id uuid NOT NULL REFERENCES users(id),
 track text NOT NULL, lesson_id text NOT NULL, question text NOT NULL, answer jsonb NOT NULL,
 created_at timestamptz NOT NULL DEFAULT now(), FOREIGN KEY(track,lesson_id) REFERENCES lessons(track,id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS exercises (
 id text PRIMARY KEY, track text NOT NULL REFERENCES courses(category), body jsonb NOT NULL,
 revision integer NOT NULL DEFAULT 1
);
CREATE TABLE IF NOT EXISTS assignments (
 id uuid PRIMARY KEY DEFAULT gen_random_uuid(), exercise_id text NOT NULL REFERENCES exercises(id),
 student_id uuid REFERENCES users(id), assigned_by uuid NOT NULL REFERENCES users(id), instructions text NOT NULL DEFAULT '',
 due_at timestamptz, closed boolean NOT NULL DEFAULT false, created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE IF NOT EXISTS submissions (
 id uuid PRIMARY KEY DEFAULT gen_random_uuid(), user_id uuid NOT NULL REFERENCES users(id),
 exercise_id text NOT NULL REFERENCES exercises(id), assignment_id uuid REFERENCES assignments(id),
 notes text NOT NULL, self_checks jsonb NOT NULL DEFAULT '[]', rubric jsonb NOT NULL,
 status text NOT NULL DEFAULT 'pending_manual' CHECK(status IN('pending_manual','passed','needs_revision')),
 result jsonb, revision integer NOT NULL DEFAULT 1, fingerprint text NOT NULL,
 created_at timestamptz NOT NULL DEFAULT now(), UNIQUE(user_id,fingerprint)
);
CREATE INDEX IF NOT EXISTS submission_owner ON submissions(user_id,created_at DESC);
CREATE TABLE IF NOT EXISTS reviews (
 id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, submission_id uuid NOT NULL REFERENCES submissions(id),
 reviewer_id uuid NOT NULL REFERENCES users(id), result jsonb NOT NULL, created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE IF NOT EXISTS files (
 id uuid PRIMARY KEY, owner_id uuid NOT NULL REFERENCES users(id), name text NOT NULL, storage_key uuid NOT NULL UNIQUE,
 content_type text NOT NULL, size bigint NOT NULL CHECK(size>0), sha256 text NOT NULL,
 submission_id uuid REFERENCES submissions(id), lesson_track text, lesson_id text,
 created_at timestamptz NOT NULL DEFAULT now(),
 FOREIGN KEY(lesson_track,lesson_id) REFERENCES lessons(track,id)
);
CREATE INDEX IF NOT EXISTS files_owner ON files(owner_id);
CREATE TABLE IF NOT EXISTS conversations (
 id uuid PRIMARY KEY DEFAULT gen_random_uuid(), user_id uuid NOT NULL REFERENCES users(id),
 track text NOT NULL REFERENCES courses(category), exercise_id text REFERENCES exercises(id),
 subject text NOT NULL, closed boolean NOT NULL DEFAULT false, revision integer NOT NULL DEFAULT 1,
 created_at timestamptz NOT NULL DEFAULT now(), updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE IF NOT EXISTS messages (
 id uuid PRIMARY KEY DEFAULT gen_random_uuid(), conversation_id uuid NOT NULL REFERENCES conversations(id),
 author_id uuid NOT NULL REFERENCES users(id), body text NOT NULL, request_id uuid NOT NULL,
 created_at timestamptz NOT NULL DEFAULT now(), UNIQUE(author_id,request_id)
);
CREATE TABLE IF NOT EXISTS competitions (
 id text PRIMARY KEY, body jsonb NOT NULL, hidden boolean NOT NULL DEFAULT false, revision integer NOT NULL DEFAULT 1,
 updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS questions_owner ON classroom_questions(user_id,track,lesson_id,created_at DESC);
CREATE INDEX IF NOT EXISTS conversation_owner ON conversations(user_id,track,updated_at DESC);
CREATE INDEX IF NOT EXISTS messages_thread ON messages(conversation_id,created_at,id);
CREATE INDEX IF NOT EXISTS files_lesson ON files(lesson_track,lesson_id);
CREATE INDEX IF NOT EXISTS files_submission ON files(submission_id);
CREATE INDEX IF NOT EXISTS reviews_submission ON reviews(submission_id,id DESC);
CREATE INDEX IF NOT EXISTS submissions_exercise ON submissions(exercise_id,created_at DESC);
