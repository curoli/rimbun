export type User = {
  id: string;
  username: string;
  display_name: string;
  email: string;
  role: string;
  created_at: string;
};

export type SiteSettings = {
  brand_name: string;
  browser_title: string;
  updated_at: string;
};

export type AuthSession = {
  user: User;
  session_token: string;
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
  has_own_text: boolean;
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

export type SubmissionSummaryDto = {
  submission_id: string;
  user_id: string;
  username: string;
  display_name: string;
  published_at: string;
  rank: number;
  support_percent: number | null;
};

export type BlockAnchorDto = {
  block_path: number[];
  heading_path: number[];
  stable_block_path: string[];
  stable_heading_path: string[];
  block_key: string;
  list_item_index: number | null;
};

export type SourceSpanDto = {
  start_line: number;
  start_column: number;
  end_line: number;
  end_column: number;
};

export type BlockVariantDto = {
  alternative_submission_id: string;
  alternative_index: number;
  kind: "unchanged" | "changed";
  weight: string | null;
  reference_text: string | null;
  reference_start: number | null;
  reference_end: number | null;
  text: string;
  source_span: SourceSpanDto | null;
};

export type CompareBlockDto = {
  block_index: number;
  block_kind: string;
  anchor: BlockAnchorDto;
  main_text: string;
  variants: BlockVariantDto[];
};

export type SectionCompareDto = {
  section_id: string;
  section_title: string;
  section_number: string;
  main_submission: SubmissionSummaryDto;
  alternatives: SubmissionSummaryDto[];
  blocks: CompareBlockDto[];
};

export type VariantCollectionRecord = {
  id: string;
  name: string;
  description: string;
  created_by: string;
  created_at: string;
  updated_at: string;
};

export type VariantEntryRecord = {
  id: string;
  collection_id: string;
  position: number;
  label: string;
  username_hint: string | null;
  markdown_content: string;
  created_at: string;
  updated_at: string;
};

export type TestRunRecord = {
  id: string;
  collection_id: string;
  document_id: string | null;
  section_id: string | null;
  status: "active" | "deleted";
  created_by: string;
  created_at: string;
  finished_at: string | null;
  deleted_at: string | null;
};

export type VariantCollectionDetail = {
  collection: VariantCollectionRecord;
  entries: VariantEntryRecord[];
  runs: TestRunRecord[];
};

export type RunCollectionResponse = {
  run: TestRunRecord;
  document: DocumentRecord;
  section: SectionRecord;
  created_users: number;
};
