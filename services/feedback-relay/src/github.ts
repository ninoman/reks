/** GitHub GraphQL calls: find the board, add a draft item, set its fields. */
import type { FieldValue, SelectField } from "./fields";
import type { Draft } from "./payload";

export interface Board {
  id: string;
  fields: SelectField[];
}

async function graphql<T>(token: string, query: string, variables: object): Promise<T> {
  const res = await fetch("https://api.github.com/graphql", {
    method: "POST",
    headers: {
      Authorization: `Bearer ${token}`,
      "User-Agent": "reks-feedback-relay",
      "Content-Type": "application/json",
    },
    body: JSON.stringify({ query, variables }),
  });
  if (!res.ok) throw new Error(`GitHub answered ${res.status}: ${await res.text()}`);
  const json = (await res.json()) as { data?: T; errors?: { message: string }[] };
  if (json.errors?.length || json.data === undefined)
    throw new Error(`GitHub: ${json.errors?.map((e) => e.message).join("; ") ?? "no data"}`);
  return json.data;
}

/** A user's board by number, with its single-select fields. */
export async function findBoard(token: string, owner: string, number: number): Promise<Board> {
  const data = await graphql<{
    user: {
      projectV2: {
        id: string;
        fields: { nodes: (Partial<SelectField> | null)[] };
      } | null;
    } | null;
  }>(
    token,
    `query($owner: String!, $number: Int!) {
      user(login: $owner) {
        projectV2(number: $number) {
          id
          fields(first: 50) {
            nodes { ... on ProjectV2SingleSelectField { id name options { id name } } }
          }
        }
      }
    }`,
    { owner, number },
  );
  const project = data.user?.projectV2;
  if (!project) throw new Error(`no project ${owner}/${number} visible to the token`);
  const fields = project.fields.nodes.filter(
    (f): f is SelectField => f?.id !== undefined && f.options !== undefined,
  );
  return { id: project.id, fields };
}

/** Adds a draft item and sets its fields; returns the item id. */
export async function addDraft(
  token: string,
  board: Board,
  draft: Draft,
  values: FieldValue[],
): Promise<string> {
  const data = await graphql<{ addProjectV2DraftIssue: { projectItem: { id: string } } }>(
    token,
    `mutation($project: ID!, $title: String!, $body: String!) {
      addProjectV2DraftIssue(input: { projectId: $project, title: $title, body: $body }) {
        projectItem { id }
      }
    }`,
    { project: board.id, title: draft.title, body: draft.body },
  );
  const item = data.addProjectV2DraftIssue.projectItem.id;
  for (const v of values) {
    await graphql(
      token,
      `mutation($project: ID!, $item: ID!, $field: ID!, $option: String!) {
        updateProjectV2ItemFieldValue(input: {
          projectId: $project, itemId: $item, fieldId: $field,
          value: { singleSelectOptionId: $option }
        }) { projectV2Item { id } }
      }`,
      { project: board.id, item, field: v.fieldId, option: v.optionId },
    );
  }
  return item;
}
