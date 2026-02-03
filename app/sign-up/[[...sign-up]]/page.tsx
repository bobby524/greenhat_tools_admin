import { SignUp } from "@clerk/nextjs";
 
export default function Page() {
  return (
    <div style={{ 
      display: 'flex', 
      justifyContent: 'center', 
      alignItems: 'center', 
      minHeight: '100vh',
      background: '#f5f5f5'
    }}>
      <SignUp />
    </div>
  );
}
