use serde::{Deserialize, Serialize};

/// Idioma activo de la interfaz. Se cambia en caliente desde la configuración.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    #[default]
    Es,
    En,
}

impl Lang {
    pub fn toggle(self) -> Self {
        match self {
            Lang::Es => Lang::En,
            Lang::En => Lang::Es,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Lang::Es => "ES",
            Lang::En => "EN",
        }
    }
}

pub fn t(lang: Lang, key: Key) -> &'static str {
    let (es, en) = pair(key);
    match lang {
        Lang::Es => es,
        Lang::En => en,
    }
}

/// Rellena los `{}` de una plantilla en orden. El texto insertado no se
/// vuelve a examinar, así un `{}` dentro de un argumento queda tal cual.
pub fn fill(template: &str, args: &[&str]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    let mut args = args.iter();
    while let Some(pos) = rest.find("{}") {
        let Some(arg) = args.next() else { break };
        out.push_str(&rest[..pos]);
        out.push_str(arg);
        rest = &rest[pos + 2..];
    }
    out.push_str(rest);
    out
}

/// Catálogo plano: una variante por texto visible, con su par (es, en). La
/// macro genera el enum y la tabla juntos para que nunca se desincronicen.
macro_rules! catalog {
    ($($name:ident => ($es:expr, $en:expr)),* $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum Key { $($name),* }

        fn pair(key: Key) -> (&'static str, &'static str) {
            match key { $(Key::$name => ($es, $en)),* }
        }

        #[cfg(test)]
        const ALL_KEYS: &[Key] = &[$(Key::$name),*];
    };
}

catalog! {
    // -- común --
    Confirm => ("confirmar", "confirm"),
    Cancel => ("cancelar", "cancel"),
    Save => ("guardar", "save"),
    NextField => ("campo", "field"),
    Loading => ("cargando…", "loading…"),
    Working => ("trabajando", "working"),
    Scanning => ("escaneando…", "scanning…"),
    Refreshing => ("Refrescando…", "Refreshing…"),
    AgoNow => ("ahora", "now"),
    Detached => ("separado", "detached"),
    Clean => ("limpio", "clean"),
    ChangesTpl => ("{} CAMBIOS", "{} CHANGES"),
    Gone => ("borrada", "gone"),
    AnyKeyClose => ("cualquier tecla cierra", "any key closes"),
    PressEnter => ("ENTER para volver a la TUI…", "Press ENTER to return to the TUI…"),
    InteractiveDone => ("De vuelta en la TUI", "Back in the TUI"),
    InteractiveExitTpl => ("El comando terminó con código {}", "The command exited with code {}"),

    // -- header / sidebar / banner --
    HeaderTitle => ("VOX · REPOSITORY", "VOX · REPOSITORY"),
    SidebarConfig => ("configuración", "settings"),
    SidebarHelp => ("ayuda", "help"),
    WsDirtyTpl => ("{} CON CAMBIOS", "{} DIRTY"),
    BannerOpTpl => ("{} EN CURSO", "{} IN PROGRESS"),
    BannerOpHint => ("M: continuar / abortar / saltar", "M: continue / abort / skip"),

    // -- pestañas --
    TabWorkspace => ("Workspace", "Workspace"),
    TabOverview => ("Resumen", "Overview"),
    TabStatus => ("Cambios", "Status"),
    TabCommits => ("Commits", "Commits"),
    TabBranches => ("Ramas", "Branches"),
    TabRemotes => ("Remotos", "Remotes"),
    TabStash => ("Stash", "Stash"),
    TabTags => ("Tags", "Tags"),
    TabWorktrees => ("Worktrees", "Worktrees"),
    TabReflog => ("Reflog", "Reflog"),
    TabSystemCheck => ("Verificación", "System check"),

    // -- columnas y paneles --
    ColRepo => ("repo", "repo"),
    ColBranch => ("rama", "branch"),
    ColChanges => ("cambios", "changes"),
    ColAge => ("edad", "age"),
    ColLastCommit => ("último commit", "last commit"),
    ColPath => ("ruta", "path"),
    ColRemote => ("remoto", "remote"),
    ColUpstream => ("upstream", "upstream"),
    ColSync => ("sincro", "sync"),
    ColFile => ("archivo", "file"),
    ColSubject => ("asunto", "subject"),
    ColAuthor => ("autor", "author"),
    ColName => ("nombre", "name"),
    ColMessage => ("mensaje", "message"),
    ColAction => ("acción", "action"),
    Detail => ("Detalle", "Detail"),
    Diff => ("Diff", "Diff"),
    Files => ("Archivos", "Files"),
    BranchLog => ("Historial de la rama", "Branch log"),
    Status => ("Estado", "Status"),
    Worktree => ("Árbol de trabajo", "Working tree"),
    Inventory => ("Inventario", "Inventory"),
    Activity => ("Actividad", "Activity"),
    ActivityTpl => ("{} commits", "{} commits"),
    Last30Days => ("30 días", "30 days"),
    Authors => ("autores", "authors"),
    RecentCommits => ("Commits recientes", "Recent commits"),
    RemoteBranches => ("Ramas remotas", "Remote branches"),
    Staged => ("indexado", "staged"),
    Unstaged => ("sin indexar", "unstaged"),
    Untracked => ("sin seguir", "untracked"),
    Conflicts => ("Conflictos", "Conflicts"),
    StatusReadoutTpl => ("+{} ~{} ?{}", "+{} ~{} ?{}"),
    CountsTpl => ("{} indexados · {} sin indexar · {} sin seguir · {} en conflicto", "{} staged · {} unstaged · {} untracked · {} conflicted"),
    TreeClean => ("Árbol limpio, nada que commitear", "Working tree clean, nothing to commit"),
    NoDetail => ("Sin contenido", "Nothing to show"),
    NoFiles => ("Sin archivos", "No files"),
    NoCommits => ("Sin commits", "No commits"),
    NoBranches => ("Sin ramas", "No branches"),
    NoRemotes => ("Sin remotos (N para agregar)", "No remotes (N to add)"),
    NoStash => ("Sin stashes", "No stashes"),
    NoTags => ("Sin tags", "No tags"),
    BinaryFile => ("Archivo binario", "Binary file"),
    RangeMode => ("RANGO", "RANGE"),
    AllBranches => ("todas las ramas", "all branches"),
    Ours => ("nuestro", "ours"),
    Theirs => ("de ellos", "theirs"),
    Both => ("ambos", "both"),
    Unresolved => ("sin resolver", "unresolved"),
    ConflictReadoutTpl => ("{} de {} pendientes", "{} of {} pending"),
    WsEmpty => ("No se encontraron repos en las raíces (F2 para configurarlas)", "No repos found under the roots (F2 to configure them)"),
    WsRootsTpl => ("Raíces: {}", "Roots: {}"),
    OnlyDirtyShort => ("SÓLO CAMBIOS", "DIRTY ONLY"),
    SortName => ("orden: nombre", "sort: name"),
    SortDirty => ("orden: cambios", "sort: changes"),
    SortRecent => ("orden: reciente", "sort: recent"),

    // -- avisos --
    NoRepo => ("No hay un repo abierto: elegilo en WORKSPACE (W)", "No repo open: pick one in WORKSPACE (W)"),
    BusyTpl => ("Hay una operación en curso: {}", "An operation is running: {}"),
    OpOkTpl => ("✔ {}", "✔ {}"),
    OpErrTpl => ("✘ {}: {}", "✘ {}: {}"),
    OpenFailedTpl => ("No se pudo abrir el repo: {}", "Could not open the repo: {}"),
    ConfigSavedTpl => ("Configuración guardada en {}", "Settings saved to {}"),
    CopiedTpl => ("Copiado: {}", "Copied: {}"),
    NoClipboard => ("No hay herramienta de portapapeles (pbcopy, wl-copy, xclip, xsel)", "No clipboard tool found (pbcopy, wl-copy, xclip, xsel)"),
    NoOpInProgress => ("No hay merge/rebase/cherry-pick en curso", "No merge/rebase/cherry-pick in progress"),
    NothingStaged => ("No hay nada indexado (A indexa todo)", "Nothing staged (A stages everything)"),
    NoCommitsYet => ("El repo todavía no tiene commits", "The repo has no commits yet"),
    NoConflictMarkers => ("El archivo no tiene marcadores de conflicto: indexalo con ESPACIO", "The file has no conflict markers: stage it with SPACE"),
    DiscardOnlyUnstaged => ("Sólo se descartan líneas de cambios sin indexar de archivos seguidos", "Only unstaged lines of tracked files can be discarded"),
    PlannerHeadOnly => ("Para reescribir historia volvé a la vista de HEAD (ESC)", "To rewrite history go back to the HEAD view (ESC)"),
    PlannerMerges => ("El rango tiene commits de merge: hacelo a mano con git rebase -r", "The range has merge commits: do it manually with git rebase -r"),
    CannotDeleteHead => ("No se puede borrar la rama actual", "Cannot delete the current branch"),
    CannotRemoveCurrentWt => ("No se puede quitar el worktree abierto", "Cannot remove the open worktree"),
    NoUpstream => ("La rama no tiene upstream", "The branch has no upstream"),
    NoRemote => ("No hay remotos configurados", "No remotes configured"),
    DetachedNoPush => ("HEAD separado: creá una rama antes de hacer push", "Detached HEAD: create a branch before pushing"),
    FieldRequired => ("Falta un campo obligatorio", "A required field is empty"),
    CommitEmpty => ("El resumen no puede estar vacío", "The summary cannot be empty"),
    LogTitle => ("Registro de comandos", "Command log"),
    LogEmpty => ("Todavía no se ejecutó ningún comando", "No commands run yet"),

    // -- descripciones de operaciones --
    DescCommit => ("commit", "commit"),
    DescAmend => ("amend", "amend"),
    DescStageAll => ("indexar todo", "stage all"),
    DescUnstageAll => ("desindexar todo", "unstage all"),
    DescStageTpl => ("indexar {}", "stage {}"),
    DescUnstageTpl => ("desindexar {}", "unstage {}"),
    DescDiscardTpl => ("descartar {}", "discard {}"),
    DescDiscardSel => ("descartar selección", "discard selection"),
    DescDiscardAll => ("descartar cambios sin indexar", "discard unstaged changes"),
    DescClean => ("borrar archivos sin seguir", "remove untracked files"),
    DescResetHard => ("reset --hard HEAD", "reset --hard HEAD"),
    DescUndoCommit => ("deshacer último commit", "undo last commit"),
    DescStash => ("stash", "stash"),
    DescIgnoreTpl => ("ignorar {}", "ignore {}"),
    DescResolvedTpl => ("resuelto {}", "resolved {}"),
    DescResolved => ("resuelto e indexado {}", "resolved and staged {}"),
    DescConflictSaved => ("guardado (con conflictos pendientes) {}", "saved (conflicts pending) {}"),
    DescReword => ("cambiar mensaje", "reword"),
    DescRebase => ("rebase", "rebase"),
    DescDrop => ("eliminar commit", "drop commit"),
    DescCheckoutTpl => ("checkout {}", "checkout {}"),
    DescResetTpl => ("reset {} {}", "reset {} {}"),
    DescRevert => ("revert", "revert"),
    DescCherryPick => ("cherry-pick", "cherry-pick"),
    DescNewBranchTpl => ("nueva rama {}", "new branch {}"),
    DescNewTagTpl => ("nuevo tag {}", "new tag {}"),
    DescAmendInto => ("enmendar commit anterior", "amend older commit"),
    DescDeleteBranchTpl => ("borrar rama {}", "delete branch {}"),
    DescRenameTpl => ("renombrar {} → {}", "rename {} → {}"),
    DescMergeTpl => ("merge {}", "merge {}"),
    DescRebaseOntoTpl => ("rebase sobre {}", "rebase onto {}"),
    DescUpstreamTpl => ("upstream {} → {}", "upstream {} → {}"),
    DescFastForwardTpl => ("fast-forward {}", "fast-forward {}"),
    DescRemoteTpl => ("remoto {}", "remote {}"),
    DescFetch => ("fetch", "fetch"),
    DescAutoFetch => ("fetch automático", "auto fetch"),
    DescPull => ("pull", "pull"),
    DescPullFf => ("pull --ff-only", "pull --ff-only"),
    DescPush => ("push", "push"),
    DescForcePush => ("push --force-with-lease", "push --force-with-lease"),
    DescStashApplyTpl => ("aplicar {}", "apply {}"),
    DescStashPopTpl => ("pop {}", "pop {}"),
    DescStashDropTpl => ("borrar {}", "drop {}"),
    DescDeleteTagTpl => ("borrar tag {}", "delete tag {}"),
    DescPushTagTpl => ("push tag {}", "push tag {}"),
    DescWorktreeTpl => ("worktree {}", "worktree {}"),
    DescPrune => ("worktree prune", "worktree prune"),
    DescContinue => ("continuar", "continue"),
    DescAbort => ("abortar", "abort"),
    DescSkip => ("saltar", "skip"),
    WsBatchTpl => ("{} en {} repos…", "{} on {} repos…"),

    // -- modales --
    ConfirmTitle => ("Confirmar", "Confirm"),
    DangerTitle => ("Acción irreversible", "Irreversible action"),
    TypeToConfirm => ("Escribí para confirmar: ", "Type to confirm: "),
    ConfirmDiscardTpl => ("¿Descartar los cambios de {}? No se puede deshacer.", "Discard the changes in {}? This cannot be undone."),
    ConfirmDiscardSel => ("¿Descartar las líneas elegidas del árbol de trabajo?", "Discard the selected lines from the working tree?"),
    ConfirmDiscardAll => ("¿Descartar todos los cambios sin indexar de archivos seguidos?", "Discard all unstaged changes to tracked files?"),
    ConfirmClean => ("Se borran todos los archivos y carpetas sin seguir (git clean -fd).", "All untracked files and directories will be deleted (git clean -fd)."),
    ConfirmHardHead => ("Se pierden todos los cambios, indexados o no (git reset --hard HEAD).", "All changes, staged or not, will be lost (git reset --hard HEAD)."),
    ConfirmUndoCommit => ("¿Deshacer el último commit dejando sus cambios indexados?", "Undo the last commit keeping its changes staged?"),
    ConfirmDropTpl => ("¿Eliminar el commit \"{}\" de la historia?", "Drop the commit \"{}\" from history?"),
    ConfirmCheckoutTpl => ("¿Hacer checkout de {}? (puede dejar HEAD separado)", "Check out {}? (may detach HEAD)"),
    ConfirmResetTpl => ("¿git reset {} a {}?", "git reset {} to {}?"),
    ConfirmRevertTpl => ("¿Crear un commit que revierta \"{}\"?", "Create a commit reverting \"{}\"?"),
    ConfirmCherryTpl => ("¿Aplicar \"{}\" sobre la rama actual?", "Apply \"{}\" on top of the current branch?"),
    ConfirmAmendIntoTpl => ("¿Meter lo indexado dentro de \"{}\"? Reescribe la historia desde ahí.", "Fold the staged changes into \"{}\"? Rewrites history from there."),
    ConfirmDeleteBranchTpl => ("¿Borrar la rama {}?", "Delete the branch {}?"),
    ConfirmForceDeleteTpl => ("Se borra la rama {} aunque tenga commits sin mergear.", "The branch {} will be deleted even with unmerged commits."),
    ConfirmDeleteRemoteTpl => ("Se borra la rama remota {} para todos.", "The remote branch {} will be deleted for everyone."),
    ConfirmMergeTpl => ("¿Hacer merge de {} en la rama actual?", "Merge {} into the current branch?"),
    ConfirmRebaseTpl => ("¿Hacer rebase de la rama actual sobre {}?", "Rebase the current branch onto {}?"),
    ConfirmRemoveRemoteTpl => ("¿Quitar el remoto {}?", "Remove the remote {}?"),
    ConfirmStashDropTpl => ("¿Borrar {}?", "Drop {}?"),
    ConfirmDeleteTagTpl => ("¿Borrar el tag local {}?", "Delete the local tag {}?"),
    ConfirmDeleteRemoteTagTpl => ("Se borra el tag {} del remoto.", "The tag {} will be deleted from the remote."),
    ConfirmRemoveWorktreeTpl => ("¿Quitar el worktree {}?", "Remove the worktree {}?"),
    ConfirmForcePush => ("El push fue rechazado. ¿Forzar con --force-with-lease?", "The push was rejected. Force it with --force-with-lease?"),
    ConfirmAbort => ("¿Abortar la operación en curso?", "Abort the operation in progress?"),
    ConfirmSkip => ("¿Saltar el commit actual?", "Skip the current commit?"),
    MenuReset => ("Descartar / reset", "Discard / reset"),
    MenuResetTo => ("Reset a este commit", "Reset to this commit"),
    MenuStash => ("Stash", "Stash"),
    MenuOp => ("Operación en curso", "Operation in progress"),
    MiDiscardUnstaged => ("descartar cambios sin indexar", "discard unstaged changes"),
    MiClean => ("borrar archivos sin seguir", "remove untracked files"),
    MiUndoCommit => ("deshacer último commit (soft)", "undo last commit (soft)"),
    MiHardReset => ("reset --hard HEAD", "reset --hard HEAD"),
    MiSoft => ("soft: conserva índice y árbol", "soft: keep index and tree"),
    MiMixed => ("mixed: conserva el árbol", "mixed: keep the tree"),
    MiHard => ("hard: descarta todo", "hard: discard everything"),
    MiStashAll => ("todo, con archivos sin seguir", "everything, including untracked"),
    MiStashStaged => ("sólo lo indexado", "staged only"),
    MiStashKeep => ("todo, manteniendo el índice", "everything, keeping the index"),
    MiContinue => ("continuar", "continue"),
    MiAbort => ("abortar", "abort"),
    MiSkip => ("saltar este commit", "skip this commit"),
    FormCommit => ("Commit", "Commit"),
    FormAmend => ("Amend", "Amend"),
    FormReword => ("Cambiar mensaje", "Reword"),
    FormStash => ("Stash", "Stash"),
    FormNewBranch => ("Nueva rama", "New branch"),
    FormNewTag => ("Nuevo tag", "New tag"),
    FormRenameBranch => ("Renombrar rama", "Rename branch"),
    FormUpstream => ("Upstream", "Upstream"),
    FormAddRemote => ("Agregar remoto", "Add remote"),
    FormRenameRemote => ("Renombrar remoto", "Rename remote"),
    FormRemoteUrl => ("URL del remoto", "Remote URL"),
    FormStashBranch => ("Rama desde stash", "Branch from stash"),
    FormAddWorktree => ("Nuevo worktree", "New worktree"),
    FormGitCommand => ("Comando git", "Git command"),
    FormGitCommandHint => ("Se ejecuta en la raíz del repo, sin terminal: nada interactivo.", "Runs at the repo root without a terminal: nothing interactive."),
    FieldMessage => ("Mensaje", "Message"),
    FieldTagMessage => ("Mensaje (anotado)", "Message (annotated)"),
    FieldName => ("Nombre", "Name"),
    FieldNewName => ("Nombre nuevo", "New name"),
    FieldUpstream => ("Upstream", "Upstream"),
    FieldUrl => ("URL", "URL"),
    FieldPath => ("Ruta", "Path"),
    FieldBranch => ("Rama", "Branch"),
    FieldMode => ("Modo", "Mode"),
    FieldSummary => ("RESUMEN", "SUMMARY"),
    FieldBody => ("DESCRIPCIÓN", "DESCRIPTION"),
    CommitEnterHint => ("commit (en el resumen) / nueva línea", "commit (in summary) / new line"),
    WtNewBranch => ("rama nueva", "new branch"),
    WtExisting => ("rama existente", "existing branch"),
    PlannerTitle => ("Rebase interactivo", "Interactive rebase"),
    PlannerHeaderTpl => ("base {} · se aplica de arriba hacia abajo", "base {} · applied top to bottom"),
    PlannerOrder => ("Más viejo arriba. Reword pide el mensaje nuevo.", "Oldest on top. Reword asks for the new message."),
    PlannerActionsShort => ("pick reword edit squash fixup drop", "pick reword edit squash fixup drop"),
    PlanEmpty => ("El plan está vacío", "The plan is empty"),
    PlanMeldFirst => ("El primer commit no puede ser squash/fixup", "The first commit cannot be squash/fixup"),
    ConfigTitle => ("Configuración", "Settings"),
    ConfigErrCreateDir => ("no se pudo crear {}", "could not create {}"),
    ConfigErrWrite => ("no se pudo escribir {}", "could not write {}"),
    ConfigErrSerialize => ("no se pudo serializar la configuración", "could not serialize the settings"),
    ConfigErrNoUserDir => ("no se pudo resolver el directorio de configuración", "could not resolve the config directory"),
    CfgRoots => ("Raíces del workspace", "Workspace roots"),
    CfgRootsHint => ("Carpetas separadas por coma (~ vale). Vacío = directorio actual.", "Comma separated folders (~ allowed). Empty = current directory."),
    CfgMaxDepth => ("Profundidad máxima", "Max depth"),
    CfgMaxDepthHint => ("Cuántos niveles bajo cada raíz se buscan repos.", "How many levels below each root are searched."),
    CfgStatusSecs => ("Refresco de estado (s)", "Status refresh (s)"),
    CfgStatusSecsHint => ("Cada cuánto se relee el estado del repo abierto.", "How often the open repo status is re-read."),
    CfgAutoFetch => ("Fetch automático (min)", "Auto fetch (min)"),
    CfgAutoFetchHint => ("git fetch --all del repo abierto cada N minutos. 0 = nunca.", "git fetch --all on the open repo every N minutes. 0 = never."),
    CfgLogLimit => ("Commits en el log", "Commits in log"),
    CfgLogLimitHint => ("Cuántos commits carga la pestaña COMMITS.", "How many commits the COMMITS tab loads."),
    CfgNetTimeout => ("Timeout de red (s)", "Network timeout (s)"),
    CfgNetTimeoutHint => ("Máximo para fetch/pull/push antes de cortar.", "Maximum for fetch/pull/push before giving up."),
    CfgLanguage => ("Idioma", "Language"),
    CfgLanguageHint => ("← → cambia entre español e inglés.", "← → switches between Spanish and English."),
    CfgErrNumber => ("{}: tiene que ser un número", "{}: must be a number"),
    CfgErrMin => ("{}: el mínimo es {}", "{}: the minimum is {}"),

    // -- barra de ayuda --
    HbNotice => (" AVISO ", " NOTICE "),
    HbView => ("sección", "section"),
    HbNav => ("mover", "move"),
    HbScroll => ("desplazar detalle", "scroll detail"),
    HbOpen => ("abrir", "open"),
    HbFetch => ("fetch", "fetch"),
    HbFetchAll => ("fetch todos", "fetch all"),
    HbPull => ("pull", "pull"),
    HbPullAll => ("pull todos", "pull all"),
    HbPush => ("push", "push"),
    HbSort => ("orden", "sort"),
    HbOnlyDirty => ("sólo con cambios", "dirty only"),
    HbFilter => ("filtrar", "filter"),
    HbApplyFilter => ("fijar filtro", "keep filter"),
    HbClearFilter => ("borrar filtro", "clear filter"),
    HbShell => ("shell", "shell"),
    HbStage => ("indexar/desindexar", "stage/unstage"),
    HbStageAll => ("todo", "all"),
    HbDiff => ("diff por línea", "line diff"),
    HbCommit => ("commit", "commit"),
    HbCommitEditor => ("commit con editor", "commit with editor"),
    HbAmend => ("amend", "amend"),
    HbDiscard => ("descartar", "discard"),
    HbResetMenu => ("reset", "reset"),
    HbStash => ("stash", "stash"),
    HbIgnore => ("ignorar", "ignore"),
    HbEdit => ("editor", "editor"),
    HbLines => ("indexar línea(s)", "stage line(s)"),
    HbHunk => ("indexar hunk", "stage hunk"),
    HbRange => ("rango", "range"),
    HbJumpHunk => ("hunk anterior/siguiente", "prev/next hunk"),
    HbToggleView => ("indexado/sin indexar", "staged/unstaged"),
    HbBack => ("volver", "back"),
    HbOurs => ("nuestro", "ours"),
    HbTheirs => ("de ellos", "theirs"),
    HbBoth => ("ambos", "both"),
    HbUnresolve => ("sin resolver", "unresolve"),
    HbWrite => ("guardar", "write"),
    HbFiles => ("archivos", "files"),
    HbCheckout => ("checkout", "checkout"),
    HbReword => ("mensaje", "reword"),
    HbSquash => ("squash", "squash"),
    HbFixup => ("fixup", "fixup"),
    HbDrop => ("eliminar", "drop"),
    HbMove => ("mover", "move"),
    HbRebaseI => ("rebase -i", "rebase -i"),
    HbAmendInto => ("enmendar aquí", "amend into"),
    HbCherry => ("cherry-pick", "cherry-pick"),
    HbRevert => ("revert", "revert"),
    HbReset => ("reset", "reset"),
    HbBranch => ("rama", "branch"),
    HbTag => ("tag", "tag"),
    HbAllBranches => ("todas las ramas", "all branches"),
    HbCopy => ("copiar", "copy"),
    HbLog => ("historial", "log"),
    HbDelete => ("borrar", "delete"),
    HbDeleteRemote => ("borrar en remoto", "delete on remote"),
    HbRename => ("renombrar", "rename"),
    HbMerge => ("merge", "merge"),
    HbRebase => ("rebase sobre", "rebase onto"),
    HbUpstream => ("upstream", "upstream"),
    HbFastForward => ("fast-forward", "fast-forward"),
    HbAdd => ("agregar", "add"),
    HbUrl => ("url", "url"),
    HbApply => ("aplicar", "apply"),
    HbPop => ("pop", "pop"),
    HbPushTag => ("push tag", "push tag"),
    HbPrune => ("prune", "prune"),
    HbRecheck => ("re-verificar", "recheck"),
    HbOpMenu => ("operación", "operation"),
    HbCmd => ("comando git", "git command"),
    HbCmdLog => ("registro", "log"),
    HbHelp => ("ayuda", "help"),
    HbQuit => ("salir", "quit"),

    // -- ayuda completa --
    HelpTitle => ("Ayuda", "Help"),
    HelpGlobal => ("Global", "Global"),
    HelpJump => ("ir a workspace / sección / verificación", "go to workspace / section / check"),
    HelpSync => ("pull / push / fetch del repo abierto", "pull / push / fetch the open repo"),
    HelpRefresh => ("refrescar", "refresh"),
    HelpWsFetch => ("fetch del repo / de todos", "fetch repo / all"),
    HelpWsPull => ("pull --ff-only del repo / de todos", "pull --ff-only repo / all"),
    HelpEnterDiff => ("entrar al diff (o al resolvedor si hay conflicto)", "enter the diff (or the resolver on conflicts)"),
    HelpCommit => ("commit / commit con $EDITOR", "commit / commit with $EDITOR"),
    HelpTakeSide => ("resolver todo el archivo con nuestro / de ellos", "resolve the whole file with ours / theirs"),
    HelpDiffFocus => ("Diff (foco)", "Diff (focused)"),
    HelpDiscardLines => ("descartar línea(s) del árbol", "discard line(s) from the tree"),
    HelpConflicts => ("Resolvedor de conflictos", "Conflict resolver"),
    HelpConflictKeys => ("nuestro / de ellos / ambos / sin resolver", "ours / theirs / both / unresolved"),
    HelpSquashFixup => ("squash / fixup con el commit anterior", "squash / fixup into the previous commit"),
    HelpPlanner => ("rebase interactivo desde este commit", "interactive rebase from this commit"),
    HelpBranchTag => ("nueva rama / tag en este commit", "new branch / tag at this commit"),
    HelpPlannerKeys => ("Rebase interactivo", "Interactive rebase"),
    HelpPlannerActions => ("pick reword edit squash fixup drop", "pick reword edit squash fixup drop"),
    HelpPlannerRun => ("ejecutar", "run"),
    HelpDeleteBranch => ("borrar / forzar", "delete / force"),
    HelpDeleteTag => ("borrar local / en el remoto", "delete local / on remote"),
    HelpDeleteWorktree => ("quitar / forzar", "remove / force"),
    HelpClose => ("↑↓ desplaza · cualquier otra tecla cierra", "↑↓ scroll · any other key closes"),

    // -- verificación del sistema --
    SetupGateTitle => ("Verificación del sistema", "System check"),
    SetupPanelTitle => ("Verificación", "System check"),
    SetupStatusAllOk => ("TODO EN ORDEN", "ALL GOOD"),
    SetupStatusActionRequired => ("REQUIERE ATENCIÓN", "ACTION REQUIRED"),
    SetupStripeAllOk => ("Todo listo", "All set"),
    SetupStripeGateBlocking => ("Falta git: sin él la TUI no puede trabajar", "git is missing: the TUI cannot work without it"),
    SetupStripePanelBlocking => ("Hay chequeos en rojo", "Some checks are red"),
    SetupKeyChoose => ("elegir", "choose"),
    SetupKeyRecheck => ("re-verificar", "recheck"),
    SetupKeyContinueAnyway => ("entrar igual", "continue anyway"),
    SetupKeyExit => ("salir", "exit"),
    DepGit => ("git", "git"),
    DepGitMissing => ("No está en el PATH: instalalo (brew install git / apt install git)", "Not in PATH: install it (brew install git / apt install git)"),
    DepGitOk => ("versión {}", "version {}"),
    DepGitOld => ("versión {}: se recomienda 2.35 o superior (stash --staged)", "version {}: 2.35 or newer recommended (stash --staged)"),
    DepIdentity => ("Identidad", "Identity"),
    DepIdentityMissing => ("Falta user.name o user.email: git config --global user.name …", "user.name or user.email missing: git config --global user.name …"),
    DepCredential => ("Credenciales", "Credentials"),
    DepCredentialNone => ("Sin credential.helper: push/fetch por HTTPS no pueden pedir contraseña", "No credential.helper: HTTPS push/fetch cannot ask for a password"),
    DepEditor => ("Editor", "Editor"),
    DepEditorNone => ("Sin $EDITOR ni core.editor: se usará vi", "No $EDITOR or core.editor: vi will be used"),
    DepSsh => ("Agente SSH", "SSH agent"),
    DepSshOk => ("SSH_AUTH_SOCK presente", "SSH_AUTH_SOCK present"),
    DepSshNone => ("Sin agente: las claves con passphrase fallarán (la TUI no puede pedirla)", "No agent: passphrase-protected keys will fail (the TUI cannot prompt)"),
    DepClipboard => ("Portapapeles", "Clipboard"),
    DepClipboardNone => ("Sin pbcopy/wl-copy/xclip/xsel: copiar (Y) no funciona", "No pbcopy/wl-copy/xclip/xsel: copy (Y) will not work"),
    DepRoots => ("Raíces", "Roots"),
    DepRootsMissing => ("No existen: {}", "Missing: {}"),
    DepConfig => ("Configuración", "Settings"),
    DepConfigDefault => ("valores por defecto (F2 para guardar)", "defaults (F2 to save)"),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_no_vuelve_a_rellenar_lo_insertado() {
        assert_eq!(fill("{} y {}", &["a{}", "b"]), "a{} y b");
        assert_eq!(fill("{} y {}", &["uno"]), "uno y {}");
        assert_eq!(fill("sin huecos", &["x"]), "sin huecos");
    }

    /// Las plantillas tienen los mismos huecos en ambos idiomas.
    #[test]
    fn plantillas_con_mismos_huecos() {
        for &k in ALL_KEYS {
            let (es, en) = pair(k);
            assert_eq!(es.matches("{}").count(), en.matches("{}").count(), "{k:?}");
            assert!(!es.is_empty() && !en.is_empty(), "{k:?}");
        }
    }
}
