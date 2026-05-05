export type User = {
  id: string;
  username: string;
  display_name: string;
  email: string;
  role: string;
  created_at: string;
};

export type DocumentRecord = {
  id: string;
  slug: string;
  title: string;
  visibility: string;
  markdown_policy: Record<string, unknown>;
  created_by: string;
  created_at: string;
};

export type SectionRecord = {
  id: string;
  document_id: string;
  parent_id: string | null;
  title: string;
  position: number;
  path: string;
  created_at: string;
};

export type SubmissionRecord = {
  id: string;
  section_id: string;
  user_id: string;
  username: string;
  display_name: string;
  base_submission_id: string | null;
  markdown_content: string;
  status: string;
  published_at: string;
  superseded_by: string | null;
};

export type DraftRecord = {
  id: string;
  section_id: string;
  user_id: string;
  base_submission_id: string | null;
  markdown_content: string;
  updated_at: string;
};

export type ProjectionItemRecord = {
  section_id: string;
  submission_id: string;
  role: string;
  rank: number;
  cluster_id: string | null;
  score: number | null;
};

export type DocumentDetailResponse = {
  document: DocumentRecord;
  sections: SectionRecord[];
};

export type SectionViewResponse = {
  section: SectionRecord;
  projection: ProjectionItemRecord[];
  active_submissions: SubmissionRecord[];
  draft: DraftRecord | null;
  preferred_base_submission_id: string | null;
};

export type PublishResponse = {
  submission: SubmissionRecord;
  queued_jobs: string[];
};
