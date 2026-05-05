import { expect, test } from "@playwright/test";

type RoutePayload = {
  method: string;
  pathname: string;
  body?: unknown;
};

function parseBody(body: string | null) {
  if (!body) {
    return null;
  }
  return JSON.parse(body);
}

test("login flow restores a session-backed UI state", async ({ page }) => {
  let meCalls = 0;

  await page.route("**/*", async (route) => {
    const url = new URL(route.request().url());
    if (!url.pathname.startsWith("/api/")) {
      await route.continue();
      return;
    }
    const payload: RoutePayload = {
      method: route.request().method(),
      pathname: url.pathname,
      body: parseBody(route.request().postData()),
    };

    if (payload.pathname === "/api/me" && payload.method === "GET") {
      meCalls += 1;
      if (meCalls === 1) {
        await route.fulfill({
          status: 401,
          contentType: "application/json",
          body: JSON.stringify({ error: "authentication required" }),
        });
        return;
      }

      await route.fulfill({
        contentType: "application/json",
        body: JSON.stringify({
          id: "user-1",
          username: "alice",
          display_name: "Alice",
          email: "alice@example.test",
          role: "normal",
          created_at: "2026-05-05T12:00:00Z",
        }),
      });
      return;
    }

    if (payload.pathname === "/api/auth/login" && payload.method === "POST") {
      expect(payload.body).toEqual({
        identifier: "alice",
        password: "secret-passphrase",
      });
      await route.fulfill({
        contentType: "application/json",
        body: JSON.stringify({
          id: "user-1",
          username: "alice",
          display_name: "Alice",
          email: "alice@example.test",
          role: "normal",
          created_at: "2026-05-05T12:00:00Z",
        }),
      });
      return;
    }

    if (payload.pathname === "/api/documents" && payload.method === "GET") {
      await route.fulfill({
        contentType: "application/json",
        body: JSON.stringify([
          {
            id: "doc-1",
            slug: "city-notes",
            title: "City Notes",
            visibility: "authenticated",
            markdown_policy: {},
            created_by: "user-1",
            created_at: "2026-05-05T12:00:00Z",
          },
        ]),
      });
      return;
    }

    await route.abort();
  });

  await page.goto("/login");
  await page.getByPlaceholder("username or email").fill("alice");
  await page.getByPlaceholder("password").fill("secret-passphrase");
  await page.locator("form").getByRole("button", { name: "Login" }).click();

  await expect(page).toHaveURL("/");
  await expect(page.getByText("Alice")).toBeVisible();
  await expect(page.getByText("@alice")).toBeVisible();
  await expect(page.getByRole("link", { name: "City Notes" })).toBeVisible();
});

test("document page can save drafts and publish updated section versions", async ({ page }) => {
  let currentView = {
    section: {
      id: "section-1",
      document_id: "doc-1",
      parent_id: null,
      title: "Opening",
      position: 0,
      path: "section-1",
      created_at: "2026-05-05T12:00:00Z",
    },
    projection: [
      {
        section_id: "section-1",
        submission_id: "submission-1",
        role: "main",
        rank: 0,
        cluster_id: null,
        score: null,
      },
    ],
    active_submissions: [
      {
        id: "submission-1",
        section_id: "section-1",
        user_id: "user-2",
        username: "bob",
        display_name: "Bob",
        base_submission_id: null,
        markdown_content: "Published baseline",
        status: "published",
        published_at: "2026-05-05T12:00:00Z",
        superseded_by: null,
      },
    ],
    draft: {
      id: "draft-1",
      section_id: "section-1",
      user_id: "user-1",
      base_submission_id: null,
      markdown_content: "Draft baseline",
      updated_at: "2026-05-05T12:01:00Z",
    },
    preferred_base_submission_id: null,
  };

  const savedDraftBodies: unknown[] = [];
  const publishBodies: unknown[] = [];

  await page.route("**/*", async (route) => {
    const url = new URL(route.request().url());
    if (!url.pathname.startsWith("/api/")) {
      await route.continue();
      return;
    }
    const payload: RoutePayload = {
      method: route.request().method(),
      pathname: url.pathname,
      body: parseBody(route.request().postData()),
    };

    if (payload.pathname === "/api/me" && payload.method === "GET") {
      await route.fulfill({
        contentType: "application/json",
        body: JSON.stringify({
          id: "user-1",
          username: "alice",
          display_name: "Alice",
          email: "alice@example.test",
          role: "normal",
          created_at: "2026-05-05T12:00:00Z",
        }),
      });
      return;
    }

    if (payload.pathname === "/api/documents" && payload.method === "GET") {
      await route.fulfill({
        contentType: "application/json",
        body: JSON.stringify([]),
      });
      return;
    }

    if (payload.pathname === "/api/documents/doc-1" && payload.method === "GET") {
      await route.fulfill({
        contentType: "application/json",
        body: JSON.stringify({
          document: {
            id: "doc-1",
            slug: "city-notes",
            title: "City Notes",
            visibility: "authenticated",
            markdown_policy: {},
            created_by: "user-1",
            created_at: "2026-05-05T12:00:00Z",
          },
          sections: [currentView.section],
        }),
      });
      return;
    }

    if (payload.pathname === "/api/sections/section-1/view" && payload.method === "GET") {
      await route.fulfill({
        contentType: "application/json",
        body: JSON.stringify(currentView),
      });
      return;
    }

    if (payload.pathname === "/api/sections/section-1/draft" && payload.method === "PUT") {
      savedDraftBodies.push(payload.body);
      currentView = {
        ...currentView,
        draft: {
          ...currentView.draft,
          markdown_content: (payload.body as { markdown_content: string }).markdown_content,
        },
      };
      await route.fulfill({
        contentType: "application/json",
        body: JSON.stringify(currentView.draft),
      });
      return;
    }

    if (payload.pathname === "/api/sections/section-1/publish" && payload.method === "POST") {
      publishBodies.push(payload.body);
      currentView = {
        ...currentView,
        projection: [
          {
            section_id: "section-1",
            submission_id: "submission-2",
            role: "main",
            rank: 0,
            cluster_id: null,
            score: null,
          },
          {
            section_id: "section-1",
            submission_id: "submission-1",
            role: "principal_alternative",
            rank: 1,
            cluster_id: null,
            score: null,
          },
        ],
        active_submissions: [
          {
            id: "submission-2",
            section_id: "section-1",
            user_id: "user-1",
            username: "alice",
            display_name: "Alice",
            base_submission_id: null,
            markdown_content: (payload.body as { markdown_content: string }).markdown_content,
            status: "published",
            published_at: "2026-05-05T12:02:00Z",
            superseded_by: null,
          },
          currentView.active_submissions[0],
        ],
        draft: {
          ...currentView.draft,
          markdown_content: (payload.body as { markdown_content: string }).markdown_content,
        },
      };

      await route.fulfill({
        contentType: "application/json",
        body: JSON.stringify({
          submission: currentView.active_submissions[0],
          queued_jobs: ["compute_embedding", "recompute_projection"],
        }),
      });
      return;
    }

    await route.abort();
  });

  await page.goto("/documents/doc-1");
  const editor = page.getByPlaceholder("Write the section content in Markdown.");
  await editor.fill("Freshly revised opening");
  await page.getByRole("button", { name: "Save Draft" }).click();
  await page.getByRole("button", { name: "Publish" }).click();

  expect(savedDraftBodies).toEqual([
    {
      base_submission_id: null,
      markdown_content: "Freshly revised opening",
    },
  ]);
  expect(publishBodies).toEqual([
    {
      base_submission_id: null,
      markdown_content: "Freshly revised opening",
    },
  ]);

  await expect(page.getByText("Alice @alice")).toBeVisible();
  await expect(page.getByText("Principal Alternatives")).toBeVisible();
  await expect(page.getByText("Bob @bob")).toBeVisible();
});

test("privileged structure controls create and move sections with computed targets", async ({ page }) => {
  const documentSections = [
    {
      id: "root-a",
      document_id: "doc-1",
      parent_id: null,
      title: "Root A",
      position: 0,
      path: "root-a",
      created_at: "2026-05-05T12:00:00Z",
    },
    {
      id: "root-b",
      document_id: "doc-1",
      parent_id: null,
      title: "Root B",
      position: 1,
      path: "root-b",
      created_at: "2026-05-05T12:01:00Z",
    },
    {
      id: "child-a1",
      document_id: "doc-1",
      parent_id: "root-a",
      title: "Child A1",
      position: 0,
      path: "root-a/child-a1",
      created_at: "2026-05-05T12:02:00Z",
    },
  ];

  const createBodies: unknown[] = [];
  const moveBodies: unknown[] = [];

  function findSection(sectionId: string) {
    return documentSections.find((section) => section.id === sectionId);
  }

  await page.route("**/*", async (route) => {
    const url = new URL(route.request().url());
    if (!url.pathname.startsWith("/api/")) {
      await route.continue();
      return;
    }
    const payload: RoutePayload = {
      method: route.request().method(),
      pathname: url.pathname,
      body: parseBody(route.request().postData()),
    };

    if (payload.pathname === "/api/me" && payload.method === "GET") {
      await route.fulfill({
        contentType: "application/json",
        body: JSON.stringify({
          id: "admin-1",
          username: "editor",
          display_name: "Editor",
          email: "editor@example.test",
          role: "privileged",
          created_at: "2026-05-05T12:00:00Z",
        }),
      });
      return;
    }

    if (payload.pathname === "/api/documents" && payload.method === "GET") {
      await route.fulfill({
        contentType: "application/json",
        body: JSON.stringify([]),
      });
      return;
    }

    if (payload.pathname === "/api/documents/doc-1" && payload.method === "GET") {
      await route.fulfill({
        contentType: "application/json",
        body: JSON.stringify({
          document: {
            id: "doc-1",
            slug: "city-notes",
            title: "City Notes",
            visibility: "authenticated",
            markdown_policy: {},
            created_by: "admin-1",
            created_at: "2026-05-05T12:00:00Z",
          },
          sections: documentSections,
        }),
      });
      return;
    }

    if (payload.pathname.startsWith("/api/sections/") && payload.pathname.endsWith("/view") && payload.method === "GET") {
      const sectionId = payload.pathname.split("/")[3];
      const section = findSection(sectionId);
      if (!section) {
        await route.abort();
        return;
      }

      await route.fulfill({
        contentType: "application/json",
        body: JSON.stringify({
          section,
          projection: [],
          active_submissions: [],
          draft: null,
          preferred_base_submission_id: null,
        }),
      });
      return;
    }

    if (payload.pathname === "/api/documents/doc-1/sections" && payload.method === "POST") {
      createBodies.push(payload.body);
      const created = {
        id: "created-child",
        document_id: "doc-1",
        parent_id: "root-b",
        title: "Inserted child",
        position: 0,
        path: "root-b/created-child",
        created_at: "2026-05-05T12:10:00Z",
      };
      documentSections.push(created);
      await route.fulfill({
        contentType: "application/json",
        body: JSON.stringify(created),
      });
      return;
    }

    if (payload.pathname.startsWith("/api/sections/") && payload.method === "PATCH") {
      moveBodies.push(payload.body);
      const sectionId = payload.pathname.split("/")[3];
      const section = findSection(sectionId);
      if (!section) {
        await route.abort();
        return;
      }

      const updated = {
        ...section,
        title: (payload.body as { title: string }).title,
        parent_id: (payload.body as { parent_id: string | null }).parent_id,
        position: (payload.body as { position: number }).position,
        path:
          (payload.body as { parent_id: string | null }).parent_id === null
            ? section.id
            : `${(payload.body as { parent_id: string }).parent_id}/${section.id}`,
      };
      const index = documentSections.findIndex((candidate) => candidate.id === sectionId);
      documentSections[index] = updated;
      await route.fulfill({
        contentType: "application/json",
        body: JSON.stringify(updated),
      });
      return;
    }

    await route.abort();
  });

  await page.goto("/documents/doc-1");

  await page.getByRole("button", { name: "Root B" }).click();
  await page.getByPlaceholder("New subsection title").fill("Inserted child");
  await page.getByRole("button", { name: "Add section" }).click();

  await page.getByRole("button", { name: "Root B" }).click();
  await page.getByRole("button", { name: "Move up" }).click();
  await page.getByRole("button", { name: "Child A1" }).click();
  await page.getByRole("button", { name: "Promote" }).click();

  expect(createBodies).toEqual([
    {
      parent_id: "root-b",
      title: "Inserted child",
      position: 0,
    },
  ]);
  expect(moveBodies).toEqual([
    {
      title: "Root B",
      parent_id: null,
      position: 0,
    },
    {
      title: "Child A1",
      parent_id: null,
      position: 1,
    },
  ]);
});
