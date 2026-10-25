import { Route, Router, type RouteSectionProps } from "@solidjs/router";
import { AuthGuard } from "./components/AuthGuard";
import { Layout } from "./components/Layout";
import { AuthProvider } from "./context/AuthContext";
import { PlayerProvider } from "./context/PlayerContext";
import { ToastProvider } from "./context/ToastContext";
import { AiAgents } from "./pages/admin/AiAgents";
import { Users } from "./pages/admin/Users";
import { Home } from "./pages/Home";
import { ItemDetail } from "./pages/ItemDetail";
import { Libraries } from "./pages/Libraries";
import { LibraryDetail } from "./pages/LibraryDetail";
import { Login } from "./pages/Login";
import { Search } from "./pages/Search";
import { Setup } from "./pages/Setup";

export function App() {
  return (
    <ToastProvider>
      <AuthProvider>
        <PlayerProvider>
          <Router>
            <Route path="/login" component={Login} />
            <Route path="/setup" component={Setup} />
            <Route
              path="/"
              component={(props: RouteSectionProps) => (
                <AuthGuard>
                  <Layout>{props.children}</Layout>
                </AuthGuard>
              )}
            >
              <Route path="/" component={Home} />
              <Route path="/libraries" component={Libraries} />
              <Route path="/library/:id" component={LibraryDetail} />
              <Route path="/item/:id" component={ItemDetail} />
              <Route path="/search" component={Search} />
              <Route
                path="/admin/users"
                component={() => (
                  <AuthGuard adminOnly>
                    <Users />
                  </AuthGuard>
                )}
              />
              <Route
                path="/admin/ai-agents"
                component={() => (
                  <AuthGuard adminOnly>
                    <AiAgents />
                  </AuthGuard>
                )}
              />
            </Route>
          </Router>
        </PlayerProvider>
      </AuthProvider>
    </ToastProvider>
  );
}

export default App;
