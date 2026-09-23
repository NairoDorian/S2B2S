import { commands } from "@/bindings";
import { sessionToast as toast } from "@/lib/sessionToast";

/** Shows a file or folder in the OS file manager; a failure is a toast. */
export const revealPath = async (path: string): Promise<void> => {
  const result = await commands.revealPathInFileManager(path);
  if (result.status === "error") toast.error(result.error);
};
