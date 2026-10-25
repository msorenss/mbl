import { Route, Router, type RouteSectionProps } from "@solidjs/router";
import { lazy } from "solid-js";
import { AuthGuard } from "./components/AuthGuard";
import { Layout } from "./components/Layout";
import { AuthProvider } from "./context/AuthContext";
import { PlayerProvider } from "./context/PlayerContext";
import { ThemeProvider } from "./context/ThemeContext";
import { ToastProvider } from "./context/ToastContext";

// Code-split pages for performance budget (M3.11)
const Home = lazy(() => import("./pages/Home"));
const ItemDetail = lazy(() => import("./pages/ItemDetail"));
const Libraries = lazy(() => import("./pages/Libraries"));
const LibraryDetail = lazy(() => import("./pages/LibraryDetail"));
const Search = lazy(() => import("./pages/Search"));
const Profiles = lazy(() => import("./pages/Profiles"));
const Login = lazy(() => import("./pages/Login"));
const Setup = lazy(() => import("./pages/Setup"));
const Users = lazy(() => import("./pages/admin/Users"));
const AiAgents = lazy(() => import("./pages/admin/AiAgents"));
const Hardware = lazy(() => import("./pages/admin/Hardware"));

export function App() {
  return (
    <ThemeProvider>
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
                <Route path="/profiles" component={Profiles} />
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
                <Route
                  path="/admin/hardware"
                  component={() => (
                    <AuthGuard adminOnly>
                      <Hardware />
                    </AuthGuard>
                  )}
                />
              </Route>
            </Router>
          </PlayerProvider>
        </AuthProvider>
      </ToastProvider>
    </ThemeProvider>
  );
}

export default App;
