import { createContext, useContext, useEffect, useState, type ReactNode } from 'react';
// @ts-ignore
import { supabase } from '@/db/supabase';
import type { User } from '@supabase/supabase-js';
// @ts-ignore
import type { Profile } from '@/types/types';
import { toast } from 'sonner';

export async function getProfile(userId: string): Promise<Profile | null> {
  const { data, error } = await supabase
    .from('profiles')
    .select('*')
    .eq('id', userId)
    .maybeSingle();

  if (error) {
    console.error('获取用户信息失败:', error);
    return null;
  }
  return data;
}
interface AuthContextType {
  user: User | null;
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
  const [user, setUser] = useState<User | null>(null);
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
    supabase
      .auth
      .getSession()
      // @ts-ignore
      .then(({ data: { session } }) => {
        setUser(session?.user ?? null);
        if (session?.user) {
          getProfile(session.user.id).then(setProfile);
        }
      })
      // @ts-ignore
      .catch(error => {
        toast.error(`获取用户信息失败: ${error.message}`);
      })
      .finally(() => {
        setLoading(false);
      });

    // @ts-ignore
    // In this function, do NOT use any await calls. Use `.then()` instead to avoid deadlocks.
    const { data: { subscription } } = supabase.auth.onAuthStateChange((_event, session) => {
      setUser(session?.user ?? null);
      if (session?.user) {
        getProfile(session.user.id).then(setProfile);
      } else {
        setProfile(null);
      }
    });

    return () => subscription.unsubscribe();
  }, []);

  const signInWithUsername = async (username: string, password: string) => {
    try {
      const email = `${username}@miaoda.com`;
      const { error } = await supabase.auth.signInWithPassword({
        email,
        password,
      });

      if (error) throw error;
      return { error: null };
    } catch (error) {
      return { error: error as Error };
    }
  };

  const signUpWithUsername = async (username: string, password: string) => {
    try {
      const email = `${username}@miaoda.com`;
      const { error } = await supabase.auth.signUp({
        email,
        password,
      });

      if (error) throw error;
      return { error: null };
    } catch (error) {
      return { error: error as Error };
    }
  };

  const mockSignIn = async (username: string) => {
    const safeUsername = username.replace(/[^a-zA-Z0-9_]/g, '_') || 'guest';
    const email = `${safeUsername}@miaoda.com`;
    const password = 'MockPass123!';

    // 尝试注册
    const { data: signUpData, error: signUpError } = await supabase.auth.signUp({
      email,
      password,
    });

    if (signUpData.user) {
      // 新用户注册成功，等待trigger创建profile
      const newUser = signUpData.user;
      setUser(newUser);
      // 延迟获取profile
      setTimeout(async () => {
        const profileData = await getProfile(newUser.id);
        setProfile(profileData);
        if (profileData) {
          localStorage.setItem('mock_user', JSON.stringify({ user: newUser, profile: profileData }));
        }
      }, 500);
      return;
    }

    // 用户已存在，尝试登录
    if (signUpError?.message?.includes('already registered') || signUpError?.message?.includes('User already registered')) {
      const { data: signInData, error: signInError } = await supabase.auth.signInWithPassword({
        email,
        password,
      });
      if (signInData.user) {
        setUser(signInData.user);
        const profileData = await getProfile(signInData.user.id);
        setProfile(profileData);
        if (profileData) {
          localStorage.setItem('mock_user', JSON.stringify({ user: signInData.user, profile: profileData }));
        }
        return;
      }
      console.error('Mock login signin error:', signInError);
    }

    // 兜底：纯本地mock模式
    console.warn('Using pure mock mode due to auth error:', signUpError);
    const mockId = `mock-${safeUsername}-${Date.now()}`;
    const mockUser = {
      id: mockId,
      email,
      app_metadata: {},
      aud: 'authenticated',
      created_at: new Date().toISOString(),
      user_metadata: { username: safeUsername },
    } as unknown as User;
    const mockProfile: Profile = {
      id: mockId,
      username: safeUsername,
      email,
      phone: null,
      role: 'user',
      created_at: new Date().toISOString(),
    };
    setUser(mockUser);
    setProfile(mockProfile);
    localStorage.setItem('mock_user', JSON.stringify({ user: mockUser, profile: mockProfile }));
  };

  // 恢复mock会话
  useEffect(() => {
    const mockData = localStorage.getItem('mock_user');
    if (mockData && !user) {
      try {
        const parsed = JSON.parse(mockData);
        if (parsed.user) {
          setUser(parsed.user as User);
          setProfile(parsed.profile as Profile);
        }
      } catch {
        localStorage.removeItem('mock_user');
      }
    }
  }, []);

  const signOut = async () => {
    await supabase.auth.signOut();
    localStorage.removeItem('mock_user');
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
