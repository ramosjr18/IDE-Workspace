// Capa de datos propia sobre la API pública de Plane (/api/v1/). No usa los stores de MobX de Plane.
// La app pone `request`: las peticiones pasan por Rust, que añade la clave; la página nunca la ve.

export type Request = (method: string, path: string, body?: unknown) => Promise<{ status: number; body: any }>;

export type StateGroup = "backlog" | "unstarted" | "started" | "completed" | "cancelled" | "triage";
export type Priority = "urgent" | "high" | "medium" | "low" | "none";

export type Project = { id: string; name: string; identifier: string };
export type State = { id: string; name: string; group: StateGroup; color: string; sequence: number };
export type Issue = { id: string; name: string; sequence_id: number; state: string; priority: Priority; sort_order: number };

export class Plane {
  constructor(private request: Request, public workspace: string) {}

  private async get<T>(path: string): Promise<T> {
    const r = await this.request("GET", `/api/v1/workspaces/${this.workspace}${path}`);
    if (r.status >= 400) throw new Error(`Plane respondió ${r.status} en ${path}`);
    return r.body as T;
  }

  // Plane pagina con cursores; para una prueba basta con pedir páginas grandes y seguir mientras haya más.
  private async all<T>(path: string): Promise<T[]> {
    const out: T[] = [];
    let cursor = "";
    for (let i = 0; i < 20; i++) {
      const sep = path.includes("?") ? "&" : "?";
      const page = await this.get<{ results: T[]; next_cursor?: string; next_page_results?: boolean }>(`${path}${sep}per_page=100${cursor ? `&cursor=${cursor}` : ""}`);
      out.push(...(page.results ?? []));
      if (!page.next_page_results || !page.next_cursor) break;
      cursor = page.next_cursor;
    }
    return out;
  }

  projects() { return this.all<Project>("/projects/"); }
  states(project: string) { return this.all<State>(`/projects/${project}/states/`); }
  issues(project: string) { return this.all<Issue>(`/projects/${project}/issues/`); }

  async setState(project: string, issue: string, state: string) {
    const r = await this.request("PATCH", `/api/v1/workspaces/${this.workspace}/projects/${project}/issues/${issue}/`, { state });
    if (r.status >= 400) throw new Error(`No se pudo cambiar el estado (${r.status})`);
    return r.body as Issue;
  }
}

// Orden de los grupos en la lista, como en Plane.
export const GROUP_ORDER: StateGroup[] = ["triage", "backlog", "unstarted", "started", "completed", "cancelled"];
