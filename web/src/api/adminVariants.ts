import { apiRequest } from "./client";
import type {
  RunCollectionResponse,
  VariantCollectionDetail,
  VariantCollectionRecord,
  VariantEntryRecord,
} from "./types";

export function listVariantCollections() {
  return apiRequest<VariantCollectionDetail[]>("/api/admin/variant-collections");
}

export function createVariantCollection(payload: { name: string; description: string }) {
  return apiRequest<VariantCollectionRecord>("/api/admin/variant-collections", {
    method: "POST",
    bodyJson: payload,
  });
}

export function updateVariantCollection(collectionId: string, payload: { name: string; description: string }) {
  return apiRequest<VariantCollectionRecord>(`/api/admin/variant-collections/${collectionId}`, {
    method: "PATCH",
    bodyJson: payload,
  });
}

export function deleteVariantCollection(collectionId: string) {
  return apiRequest<{ status: string }>(`/api/admin/variant-collections/${collectionId}`, {
    method: "DELETE",
  });
}

export function createVariantEntry(
  collectionId: string,
  payload: {
    markdown_content: string;
  },
) {
  return apiRequest<VariantEntryRecord>(`/api/admin/variant-collections/${collectionId}/entries`, {
    method: "POST",
    bodyJson: payload,
  });
}

export function updateVariantEntry(
  entryId: string,
  payload: {
    markdown_content: string;
  },
) {
  return apiRequest<VariantEntryRecord>(`/api/admin/variant-entries/${entryId}`, {
    method: "PATCH",
    bodyJson: payload,
  });
}

export function deleteVariantEntry(entryId: string) {
  return apiRequest<{ status: string }>(`/api/admin/variant-entries/${entryId}`, {
    method: "DELETE",
  });
}

export function runVariantCollection(collectionId: string) {
  return apiRequest<RunCollectionResponse>(`/api/admin/variant-collections/${collectionId}/test-runs`, {
    method: "POST",
  });
}

export function deleteTestRun(runId: string) {
  return apiRequest<{ status: string }>(`/api/admin/test-runs/${runId}`, {
    method: "DELETE",
  });
}
