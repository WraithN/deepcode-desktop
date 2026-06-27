import LoginPage from './pages/LoginPage';
import SelectAgentPage from './pages/SelectAgentPage';
import WorkspacePage from './pages/WorkspacePage';
import DashboardPage from './pages/DashboardPage';
import type { ReactNode } from 'react';

console.log("[routes.tsx] Loading route modules...");

export interface RouteConfig {
  name: string;
  path: string;
  element: ReactNode;
  visible?: boolean;
  /** Accessible without login. Routes without this flag require authentication. Has no effect when RouteGuard is not in use. */
  public?: boolean;
}

export const routes: RouteConfig[] = [
  {
    name: '登录',
    path: '/login',
    element: <LoginPage />,
    public: true,
  },
  {
    name: '选择智能体',
    path: '/select-agent',
    element: <SelectAgentPage />,
  },
  {
    name: '工作区',
    path: '/workspace',
    element: <WorkspacePage />,
  },
  {
    name: '数据大盘',
    path: '/dashboard',
    element: <DashboardPage />,
  },
  {
    name: '首页',
    path: '/',
    element: <LoginPage />,
    public: true,
  },
];

console.log("[routes.tsx] Routes defined:", routes.map(r => r.path));
