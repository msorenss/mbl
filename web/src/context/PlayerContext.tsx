import { createContext, createSignal, type JSX, lazy, Show, useContext } from "solid-js";

const Player = lazy(() => import("../components/Player"));

export interface PlayRequest {
  itemId: number;
  initialStartMs?: number;
  title?: string;
  subtitle?: string;
}

interface PlayerContextType {
  playItem: (req: PlayRequest) => void;
  closePlayer: () => void;
  activeItem: () => PlayRequest | null;
}

const PlayerContext = createContext<PlayerContextType>();

export function PlayerProvider(props: { children: JSX.Element }) {
  const [activeItem, setActiveItem] = createSignal<PlayRequest | null>(null);

  const playItem = (req: PlayRequest) => {
    setActiveItem(req);
  };

  const closePlayer = () => {
    setActiveItem(null);
  };

  return (
    <PlayerContext.Provider value={{ playItem, closePlayer, activeItem }}>
      {props.children}
      <Show when={activeItem()}>
        {(item) => (
          <Player
            itemId={item().itemId}
            initialStartMs={item().initialStartMs}
            title={item().title}
            subtitle={item().subtitle}
            onClose={closePlayer}
          />
        )}
      </Show>
    </PlayerContext.Provider>
  );
}

export function usePlayer() {
  const context = useContext(PlayerContext);
  if (!context) {
    throw new Error("usePlayer must be used within a PlayerProvider");
  }
  return context;
}
