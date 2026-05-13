use event_opcode::EventOpCode;
pub enum OpCode {
    TestEvent,
}
pub trait EventOpCode {
    fn op_code(&self) -> OpCode;
}
#[derive(EventOpCode)]
pub struct TestEventReq;
fn main() {
    let req = TestEventReq {};
    let _code = req.op_code();
}
