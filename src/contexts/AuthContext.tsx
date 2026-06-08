import { createContext, useContext, useEffect, useState, type ReactNode } from 'react';
import { db, type AuthUser } from '@/db';
import type { Profile } from '@/types/types';
import { toast } from 'sonner';

export async function getProfile(userId: string): Promise<Profile | null> {
  return db.getProfile(userId);
}

interface AuthContextType {
  user: AuthUser | null;
  profile: Profile | null;
  loading: boolean;
  signInWithUsername: (username: string, password: string) => Promise<{ error: Error | null }>;
  signUpWithUsername: (username: string, password: string) => Promise<{ error: Error | null }>;
  mockSignIn: (username: string) => Promise<void>;
  signOut: () => Promise<void>;
  refreshProfile: () => Promise<void>;
}

const AuthContext = createContext<AuthContextType | undefined>(undefined);

export function AuthProvider({ children }: { children: ReactNode }) {
  console.log("[AuthContext.tsx] AuthProvider rendering...");
  const [user, setUser] = useState<AuthUser | null>(null);
  const [profile, setProfile] = useState<Profile | null>(null);
  const [loading, setLoading] = useState(true);

  const refreshProfile = async () => {
    if (!user) {
      setProfile(null);
      return;
    }
    const profileData = await getProfile(user.id);
    setProfile(profileData);
  };

  useEffect(() => {
    db.getSession()
      .then((session) => {
        setUser(session?.user ?? null);
        if (session?.user) {
          getProfile(session.user.id).then(setProfile);
        }
      })
      .catch((error) => {
        toast.error(`获取用户信息失败: ${error.message}`);
      })
      .finally(() => {
        setLoading(false);
      });

    const { unsubscribe } = db.onAuthStateChange((_event, session) => {
      setUser(session?.user ?? null);
      if (session?.user) {
        getProfile(session.user.id).then(setProfile);
      } else {
        setProfile(null);
      }
    });

    return () => unsubscribe();
  }, []);

  const signInWithUsername = async (username: string, password: string) => {
    try {
      const { error } = await db.signIn(username, password);
      if (error) { throw error; }
      return { error: null };
    } catch (error) {
      return { error: error as Error };
    }
  };

  const signUpWithUsername = async (username: string, password: string) => {
    try {
      const { error } = await db.signUp(username, password);
      if (error) { throw error; }
      return { error: null };
    } catch (error) {
      return { error: error as Error };
    }
  };

  const mockSignIn = async (username: string) => {
    const safeUsername = username.replace(/[^a-zA-Z0-9_]/g, '_') || 'guest';

    // 先尝试注册
    const { user: signUpUser, error: signUpError } = await db.signUp(safeUsername, 'MockPass123!');

    if (signUpUser) {
      setUser(signUpUser);
      setTimeout(async () => {
        const profileData = await getProfile(signUpUser.id);
        setProfile(profileData);
      }, 100);
      return;
    }

    // 用户已存在，尝试登录
    if (signUpError?.message?.includes('已存在') || signUpError?.message?.includes('already')) {
      const { user: signInUser, error: signInError } = await db.signIn(safeUsername, 'MockPass123!');
      if (signInUser) {
        setUser(signInUser);
        const profileData = await getProfile(signInUser.id);
        setProfile(profileData);
        return;
      }
      console.error('Mock login signin error:', signInError);
    }

    // 兜底：使用内存 mock 模式（如果 db 不可用）
    console.warn('Using pure mock mode due to auth error:', signUpError);
    const mockId = `mock-${safeUsername}-${Date.now()}`;
    const mockUser: AuthUser = {
      id: mockId,
      email: `${safeUsername}@local.dev`,
      username: safeUsername,
      created_at: new Date().toISOString(),
    };
    const mockProfile: Profile = {
      id: mockId,
      username: safeUsername,
      email: `${safeUsername}@local.dev`,
      phone: null,
      role: 'user',
      created_at: new Date().toISOString(),
    };
    setUser(mockUser);
    setProfile(mockProfile);
    localStorage.setItem('aicoding_mock_user', JSON.stringify({ user: mockUser, profile: mockProfile }));
  };

  // 恢复 mock 会话
  useEffect(() => {
    const mockData = localStorage.getItem('aicoding_mock_user');
    if (mockData && !user) {
      try {
        const parsed = JSON.parse(mockData);
        if (parsed.user) {
          setUser(parsed.user as AuthUser);
          setProfile(parsed.profile as Profile);
        }
      } catch {
        localStorage.removeItem('aicoding_mock_user');
      }
    }
  }, []);

  const signOut = async () => {
    await db.signOut();
    localStorage.removeItem('aicoding_mock_user');
    setUser(null);
    setProfile(null);
  };

  return (
    <AuthContext.Provider value={{ user, profile, loading, signInWithUsername, signUpWithUsername, mockSignIn, signOut, refreshProfile }}>
      {children}
    </AuthContext.Provider>
  );
}

export function useAuth() {
  const context = useContext(AuthContext);
  if (context === undefined) {
    throw new Error('useAuth must be used within an AuthProvider');
  }
  return context;
}
