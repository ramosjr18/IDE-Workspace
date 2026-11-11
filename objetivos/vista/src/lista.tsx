// Lista de issues agrupada por estado, con el marcado y las clases de la vista de lista de Plane
// (objetivos/plane/apps/web/components/issues/issue-layouts/list/{list-group,block}.tsx y
// headers/group-by-card.tsx, AGPL-3.0, Plane Software, Inc. and contributors) y sus piezas reales
// (Row, StateGroupIcon, PriorityIcon). Los datos vienen de nuestra capa (api.ts), no de sus stores.
import { useCallback, useEffect, useMemo, useState } from "react";
import { Row } from "@plane/blocks/layout";
import { StateGroupIcon, PriorityIcon } from "@plane/blocks/icons";
import { ChevronRightOutline } from "@makeplane/propel/icons";
import { cn } from "@plane/utils";
import { GROUP_ORDER, type Issue, type Plane, type Project, type State } from "./api";

type Props = { plane: Plane };

export function Lista({ plane }: Props) {
  const [projects, setProjects] = useState<Project[] | null>(null);
  const [project, setProject] = useState<string>("");
  const [states, setStates] = useState<State[]>([]);
  const [issues, setIssues] = useState<Issue[]>([]);
  const [error, setError] = useState<string>("");
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({});

  useEffect(() => {
    plane.projects().then((p) => {
      setProjects(p);
      setProject((cur) => cur || p[0]?.id || "");
    }, (e) => setError(String(e?.message ?? e)));
  }, [plane]);

  const load = useCallback(async () => {
    if (!project) return;
    try {
      const [s, i] = await Promise.all([plane.states(project), plane.issues(project)]);
      setStates(s);
      setIssues(i);
      setError("");
    } catch (e: any) {
      setError(String(e?.message ?? e));
    }
  }, [plane, project]);

  useEffect(() => { load(); }, [load]);

  const current = projects?.find((p) => p.id === project);
  // Grupos: un estado por grupo de la lista, en el orden de Plane (grupo y luego secuencia).
  const groups = useMemo(() => {
    const ordered = [...states].sort((a, b) => GROUP_ORDER.indexOf(a.group) - GROUP_ORDER.indexOf(b.group) || a.sequence - b.sequence);
    return ordered.map((s) => ({ state: s, items: issues.filter((i) => i.state === s.id).sort((a, b) => a.sort_order - b.sort_order) }));
  }, [states, issues]);
  const stateOf = (id: string) => states.find((s) => s.id === id);

  if (error) return <div className="p-4 text-body-xs-regular text-danger-primary">{error}</div>;
  if (!projects) return <div className="p-4 text-body-xs-regular text-tertiary">Cargando…</div>;
  if (!projects.length) return <div className="p-4 text-body-xs-regular text-tertiary">No hay proyectos en Plane.</div>;

  return (
    <div className="flex h-full w-full flex-col bg-surface-1 text-primary">
      {/* Cabecera: proyecto */}
      <div className="flex h-11 flex-shrink-0 items-center gap-2 border-b border-subtle px-page-x">
        <select
          className="rounded-sm border border-subtle bg-layer-1 px-2 py-1 text-body-xs-medium text-primary outline-none"
          value={project}
          onChange={(e) => setProject(e.target.value)}
        >
          {projects.map((p) => (
            <option key={p.id} value={p.id}>{p.name}</option>
          ))}
        </select>
        <span className="text-body-xs-regular text-tertiary">{issues.length} work items</span>
        <button type="button" className="ml-auto rounded-sm px-2 py-1 text-body-xs-medium text-secondary hover:bg-layer-1-hover" onClick={load}>
          Actualizar
        </button>
      </div>

      <div className="relative h-full w-full overflow-auto">
        {groups.map(({ state, items }) => (
          <div key={state.id} className="relative flex flex-shrink-0 flex-col">
            {/* Cabecera del grupo (group-by-card.tsx) */}
            <div className="w-full flex-shrink-0 border-b border-subtle bg-layer-1 py-1 pr-5 hover:bg-layer-1-hover">
              <Row>
                <div className="group/list-header flex w-full flex-shrink-0 items-center gap-2 py-1.5">
                  <div className="grid flex-shrink-0 place-items-center overflow-hidden">
                    <StateGroupIcon stateGroup={state.group} color={state.color} size="md" />
                  </div>
                  <button
                    type="button"
                    className="relative flex w-full min-w-0 cursor-pointer flex-row items-center gap-1 rounded-xs text-left outline-none focus-visible:ring-2 focus-visible:ring-accent-strong"
                    onClick={() => setCollapsed((c) => ({ ...c, [state.id]: !c[state.id] }))}
                  >
                    <span className="line-clamp-1 inline-block truncate text-body-sm-medium text-primary">{state.name}</span>
                    <span className="pl-2 text-body-xs-medium text-tertiary">{items.length}</span>
                  </button>
                </div>
              </Row>
            </div>

            {/* Filas (block.tsx) */}
            {!collapsed[state.id] &&
              items.map((issue) => (
                <div key={issue.id} className="w-full cursor-pointer border-b border-subtle">
                  <Row className="group/list-block relative flex min-h-11 flex-col gap-3 bg-layer-transparent py-3 text-body-xs-regular transition-colors hover:bg-layer-transparent-hover md:flex-row md:items-center">
                    <div className="flex w-full gap-2 truncate">
                      <div className="flex flex-grow items-center gap-0.5 truncate">
                        <div className="flex items-center gap-1">
                          <div className="flex-shrink-0" style={{ minWidth: "52px" }}>
                            <span className="text-body-xs-medium text-tertiary">
                              {current?.identifier}-{issue.sequence_id}
                            </span>
                          </div>
                          <div className="grid size-4 flex-shrink-0 place-items-center" />
                        </div>
                        <p className="cursor-pointer truncate text-body-xs-medium text-primary" title={issue.name}>{issue.name}</p>
                      </div>
                    </div>
                    {/* Propiedades: estado y prioridad (cambiar el estado escribe en Plane) */}
                    <div className="flex flex-shrink-0 items-center gap-2">
                      <label className={cn("relative flex h-5 items-center gap-1 rounded-sm border-[0.5px] border-strong px-1.5 text-caption-sm-regular text-secondary hover:bg-layer-1-hover")}>
                        <StateGroupIcon stateGroup={stateOf(issue.state)?.group ?? "backlog"} color={stateOf(issue.state)?.color} size="sm" />
                        <select
                          className="cursor-pointer appearance-none bg-transparent pr-1 outline-none"
                          value={issue.state}
                          onChange={async (e) => {
                            const s = e.target.value;
                            setIssues((all) => all.map((x) => (x.id === issue.id ? { ...x, state: s } : x)));
                            try { await plane.setState(project, issue.id, s); } catch (err: any) { setError(String(err?.message ?? err)); }
                          }}
                        >
                          {states.map((s) => (
                            <option key={s.id} value={s.id}>{s.name}</option>
                          ))}
                        </select>
                      </label>
                      <span className="grid h-5 w-5 place-items-center rounded-sm border-[0.5px] border-strong" title={issue.priority}>
                        <PriorityIcon priority={issue.priority} className="size-3" />
                      </span>
                      <ChevronRightOutline className="size-3.5 text-placeholder" />
                    </div>
                  </Row>
                </div>
              ))}
          </div>
        ))}
      </div>
    </div>
  );
}
