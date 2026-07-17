import { apiRequest } from "./client";
import type {
  DocumentDetailResponse,
  DocumentRecord,
  DraftRecord,
  PublishResponse,
  CommentRecord,
  SectionRecord,
  SectionCompareDto,
  SectionViewResponse,
} from "./types";

export function listDocuments() {
  return apiRequest<DocumentRecord[]>("/api/documents");
}

export function getDocument(documentRef: string) {
  return apiRequest<DocumentDetailResponse>(`/api/documents/${documentRef}`);
}

export function updateDocument(
  documentId: string,
  payload: {
    slug: string;
    title: string;
    visibility: "public" | "authenticated";
    markdown_policy?: Record<string, unknown>;
  },
) {
  return apiRequest<DocumentRecord>(`/api/documents/${documentId}`, {
    method: "PATCH",
    bodyJson: payload,
  });
}

export function createDocument(payload: {
  slug: string;
  title: string;
  visibility: "public" | "authenticated";
  markdown_policy?: Record<string, unknown>;
}) {
  return apiRequest<DocumentRecord>("/api/documents", {
    method: "POST",
    bodyJson: payload,
  });
}

export function createSection(
  documentId: string,
  payload: {
    parent_id: string | null;
    title: string;
    has_heading: boolean;
    has_own_text: boolean;
    position: number;
  },
) {
  return apiRequest<SectionRecord>(`/api/documents/${documentId}/sections`, {
    method: "POST",
    bodyJson: payload,
  });
}

export function updateSection(
  sectionId: string,
  payload: {
    title: string;
    has_heading: boolean;
    has_own_text: boolean;
    parent_id: string | null;
    position: number;
  },
) {
  return apiRequest<SectionRecord>(`/api/sections/${sectionId}`, {
    method: "PATCH",
    bodyJson: payload,
  });
}

export function deleteSection(sectionId: string) {
  return apiRequest<void>(`/api/sections/${sectionId}`, {
    method: "DELETE",
  });
}

export function getSectionView(sectionId: string) {
  return apiRequest<SectionViewResponse>(`/api/sections/${sectionId}/view`);
}

export function getSectionCompare(sectionId: string) {
  return apiRequest<SectionCompareDto>(`/api/sections/${sectionId}/compare`);
}

export function saveDraft(sectionId: string, payload: {
  base_submission_id: string | null;
  markdown_content: string;
  main_comment_markdown: string | null;
}) {
  return apiRequest<DraftRecord>(`/api/sections/${sectionId}/draft`, {
    method: "PUT",
    bodyJson: payload,
  });
}

export function publishSection(sectionId: string, payload: {
  base_submission_id: string | null;
  markdown_content: string;
  main_comment_markdown: string | null;
}) {
  return apiRequest<PublishResponse>(`/api/sections/${sectionId}/publish`, {
    method: "POST",
    bodyJson: payload,
  });
}

export function createSubmissionComment(submissionId: string, payload: {
  parent_comment_id: string | null;
  markdown_content: string;
  is_primary?: boolean;
}) {
  return apiRequest<CommentRecord>(`/api/submissions/${submissionId}/comments`, {
    method: "POST",
    bodyJson: payload,
  });
}

export function deleteSubmission(submissionId: string) {
  return apiRequest<void>(`/api/submissions/${submissionId}`, {
    method: "DELETE",
  });
}

export function deleteComment(commentId: string) {
  return apiRequest<void>(`/api/comments/${commentId}`, {
    method: "DELETE",
  });
}

export function setPreferredBase(sectionId: string, preferredBaseSubmissionId: string) {
  return apiRequest<{ preferred_base_submission_id: string }>(
    `/api/sections/${sectionId}/preferences/base-submission`,
    {
      method: "PUT",
      bodyJson: { preferred_base_submission_id: preferredBaseSubmissionId },
    },
  );
}
