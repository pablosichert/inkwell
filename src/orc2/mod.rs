pub mod lljit;

use std::{
    ffi::CStr,
    fmt,
    iter::FromIterator,
    marker::PhantomData,
    mem::{forget, transmute},
    ptr, slice, vec,
};

use libc::c_void;
#[llvm_versions(12.0..=latest)]
use llvm_sys::orc2::{
    ee::LLVMOrcCreateRTDyldObjectLinkingLayerWithSectionMemoryManager, LLVMJITCSymbolMapPair,
    LLVMJITEvaluatedSymbol, LLVMJITSymbolFlags, LLVMOrcAbsoluteSymbols, LLVMOrcCSymbolMapPairs,
    LLVMOrcDisposeMaterializationUnit, LLVMOrcDisposeObjectLayer,
    LLVMOrcExecutionSessionCreateBareJITDylib, LLVMOrcExecutionSessionCreateJITDylib,
    LLVMOrcExecutionSessionGetJITDylibByName, LLVMOrcJITDylibClear,
    LLVMOrcJITDylibCreateResourceTracker, LLVMOrcJITDylibDefine,
    LLVMOrcJITDylibGetDefaultResourceTracker, LLVMOrcMaterializationUnitRef, LLVMOrcObjectLayerRef,
    LLVMOrcReleaseResourceTracker, LLVMOrcResourceTrackerRef, LLVMOrcResourceTrackerRemove,
    LLVMOrcResourceTrackerTransferTo, LLVMOrcRetainSymbolStringPoolEntry,
    LLVMOrcSymbolStringPoolEntryStr,
};
#[llvm_versions(13.0..=latest)]
use llvm_sys::orc2::{
    LLVMOrcCDependenceMapPair, LLVMOrcCDependenceMapPairs, LLVMOrcCSymbolAliasMapPair,
    LLVMOrcCSymbolFlagsMapPair, LLVMOrcCSymbolFlagsMapPairs, LLVMOrcCSymbolsList,
    LLVMOrcCreateCustomMaterializationUnit, LLVMOrcDisposeSymbols, LLVMOrcIRTransformLayerEmit,
    LLVMOrcIRTransformLayerRef, LLVMOrcIndirectStubsManagerRef,
    LLVMOrcJITTargetMachineBuilderGetTargetTriple, LLVMOrcJITTargetMachineBuilderSetTargetTriple,
    LLVMOrcLazyCallThroughManagerRef, LLVMOrcLazyReexports,
    LLVMOrcMaterializationResponsibilityAddDependencies,
    LLVMOrcMaterializationResponsibilityAddDependenciesForAll,
    LLVMOrcMaterializationResponsibilityDefineMaterializing,
    LLVMOrcMaterializationResponsibilityDelegate,
    LLVMOrcMaterializationResponsibilityFailMaterialization,
    LLVMOrcMaterializationResponsibilityGetExecutionSession,
    LLVMOrcMaterializationResponsibilityGetInitializerSymbol,
    LLVMOrcMaterializationResponsibilityGetRequestedSymbols,
    LLVMOrcMaterializationResponsibilityGetSymbols,
    LLVMOrcMaterializationResponsibilityGetTargetDylib,
    LLVMOrcMaterializationResponsibilityNotifyEmitted,
    LLVMOrcMaterializationResponsibilityNotifyResolved, LLVMOrcMaterializationResponsibilityRef,
    LLVMOrcMaterializationResponsibilityReplace, LLVMOrcObjectLayerAddObjectFile,
    LLVMOrcObjectLayerEmit,
};
use llvm_sys::orc2::{
    LLVMOrcCreateNewThreadSafeContext, LLVMOrcCreateNewThreadSafeModule,
    LLVMOrcDisposeJITTargetMachineBuilder, LLVMOrcDisposeThreadSafeContext,
    LLVMOrcDisposeThreadSafeModule, LLVMOrcExecutionSessionIntern, LLVMOrcExecutionSessionRef,
    LLVMOrcJITDylibRef, LLVMOrcJITTargetMachineBuilderCreateFromTargetMachine,
    LLVMOrcJITTargetMachineBuilderDetectHost, LLVMOrcJITTargetMachineBuilderRef,
    LLVMOrcReleaseSymbolStringPoolEntry, LLVMOrcSymbolStringPoolEntryRef,
    LLVMOrcThreadSafeContextGetContext, LLVMOrcThreadSafeContextRef, LLVMOrcThreadSafeModuleRef,
};
// #[llvm_versions(14.0..=latest)]
// use llvm_sys::orc2::LLVMOrcObjectLayerAddObjectFileWithRT;

use crate::{
    context::Context,
    error::LLVMError,
    memory_buffer::MemoryBuffer,
    module::Module,
    support::{to_c_str, LLVMString, OwnedOrBorrowedPtr},
    targets::TargetMachine,
};

/// The thread safe variant of [`Context`] used in this module.
#[derive(Debug)]
pub struct ThreadSafeContext {
    thread_safe_context: LLVMOrcThreadSafeContextRef,
    context: Context,
}

impl ThreadSafeContext {
    unsafe fn new(thread_safe_context: LLVMOrcThreadSafeContextRef) -> Self {
        assert!(!thread_safe_context.is_null());
        let context = Context::new(LLVMOrcThreadSafeContextGetContext(thread_safe_context));
        ThreadSafeContext {
            thread_safe_context,
            context,
        }
    }

    /// Creates a new `ThreadSafeContext`.
    /// ```
    /// use inkwell::orc2::ThreadSafeContext;
    ///
    /// let thread_safe_context = ThreadSafeContext::create();
    /// ```
    pub fn create() -> Self {
        unsafe { ThreadSafeContext::new(LLVMOrcCreateNewThreadSafeContext()) }
    }

    /// Gets the [`Context`] from this instance. The returned [`Context`]
    /// can be used for most operations concerning the context.
    /// ```
    /// use inkwell::orc2::ThreadSafeContext;
    ///
    /// let thread_safe_context = ThreadSafeContext::create();
    /// let context = thread_safe_context.context();
    /// ```
    pub fn context(&self) -> &Context {
        &self.context
    }

    /// Creates a [`ThreadSafeModule`] from a [`Module`]. The [`Module`] is now
    /// owned by the [`ThreadSafeModule`]
    /// ```
    /// use inkwell::orc2::ThreadSafeContext;
    ///
    /// let thread_safe_context = ThreadSafeContext::create();
    /// let context = thread_safe_context.context();
    /// let module = context.create_module("main");
    /// let thread_safe_module = thread_safe_context.create_module(module);
    /// ```
    pub fn create_module<'ctx>(&'ctx self, module: Module<'ctx>) -> ThreadSafeModule<'ctx> {
        unsafe {
            ThreadSafeModule::new(
                LLVMOrcCreateNewThreadSafeModule(module.module.get(), self.thread_safe_context),
                module,
            )
        }
    }
}
impl Drop for ThreadSafeContext {
    fn drop(&mut self) {
        unsafe {
            LLVMOrcDisposeThreadSafeContext(self.thread_safe_context);
        }
        self.context.context = ptr::null_mut(); // context is already disposed
    }
}

/// The thread safe variant of [`Module`] used in this module. The underlying
/// llvm module will be disposed when this object is dropped, except when
/// it is owned by an execution engine.
#[derive(Debug)]
pub struct ThreadSafeModule<'ctx> {
    thread_safe_module: LLVMOrcThreadSafeModuleRef,
    module: Module<'ctx>,
}

impl<'ctx> ThreadSafeModule<'ctx> {
    unsafe fn new(thread_safe_module: LLVMOrcThreadSafeModuleRef, module: Module<'ctx>) -> Self {
        assert!(!thread_safe_module.is_null());
        ThreadSafeModule {
            thread_safe_module,
            module,
        }
    }
}
impl<'ctx> Drop for ThreadSafeModule<'ctx> {
    fn drop(&mut self) {
        unsafe {
            LLVMOrcDisposeThreadSafeModule(self.thread_safe_module);
        }
        self.module.module.set(ptr::null_mut()); // module is already disposed
    }
}

#[derive(Debug)]
pub struct JITTargetMachineBuilder {
    builder: LLVMOrcJITTargetMachineBuilderRef,
}

impl JITTargetMachineBuilder {
    unsafe fn new(builder: LLVMOrcJITTargetMachineBuilderRef) -> Self {
        assert!(!builder.is_null());
        JITTargetMachineBuilder { builder }
    }

    pub fn detect_host() -> Result<Self, LLVMError> {
        let mut builder = ptr::null_mut();
        unsafe {
            LLVMError::new(LLVMOrcJITTargetMachineBuilderDetectHost(&mut builder))?;
            Ok(JITTargetMachineBuilder::new(builder))
        }
    }

    #[llvm_versions(12.0..=latest)]
    pub fn create_from_target_machine(target_machine: TargetMachine) -> Self {
        let jit_target_machine_builder = unsafe {
            JITTargetMachineBuilder::new(LLVMOrcJITTargetMachineBuilderCreateFromTargetMachine(
                target_machine.target_machine,
            ))
        };
        forget(target_machine);
        jit_target_machine_builder
    }

    #[llvm_versions(13.0..=latest)]
    pub fn get_target_triple(&self) -> LLVMString {
        unsafe { LLVMString::new(LLVMOrcJITTargetMachineBuilderGetTargetTriple(self.builder)) }
    }

    #[llvm_versions(13.0..=latest)]
    pub fn set_target_triple(&self, target_triple: &str) {
        unsafe {
            LLVMOrcJITTargetMachineBuilderSetTargetTriple(
                self.builder,
                to_c_str(target_triple).as_ptr(),
            )
        }
    }
}

impl Drop for JITTargetMachineBuilder {
    fn drop(&mut self) {
        unsafe {
            LLVMOrcDisposeJITTargetMachineBuilder(self.builder);
        }
    }
}

/// Represents a dynamic library in the execution engine.
#[derive(Debug)]
#[repr(transparent)]
pub struct JITDylib<'jit> {
    jit_dylib: LLVMOrcJITDylibRef,
    _marker: PhantomData<&'jit ()>,
}

impl<'jit> JITDylib<'jit> {
    unsafe fn new(jit_dylib: LLVMOrcJITDylibRef) -> Self {
        assert!(!jit_dylib.is_null());
        JITDylib {
            jit_dylib,
            _marker: PhantomData,
        }
    }

    /// Returns the default [`ResourceTracker`].
    /// ```
    /// use inkwell::orc2::lljit::LLJIT;
    ///
    /// let lljit = LLJIT::create().expect("LLJIT::create failed");
    /// let main_jd = lljit.get_main_jit_dylib();
    /// let rt = main_jd.get_default_resource_tracker();
    /// ```
    #[llvm_versions(12.0..=latest)]
    pub fn get_default_resource_tracker(&self) -> ResourceTracker<'jit> {
        unsafe {
            ResourceTracker::new(
                LLVMOrcJITDylibGetDefaultResourceTracker(self.jit_dylib),
                true,
            )
        }
    }

    /// Creates a new [`ResourceTracker`].
    /// ```
    /// use inkwell::orc2::lljit::LLJIT;
    ///
    /// let lljit = LLJIT::create().expect("LLJIT::create failed");
    /// let main_jd = lljit.get_main_jit_dylib();
    /// let rt = main_jd.create_resource_tracker();
    /// ```
    #[llvm_versions(12.0..=latest)]
    pub fn create_resource_tracker(&self) -> ResourceTracker<'jit> {
        unsafe { ResourceTracker::new(LLVMOrcJITDylibCreateResourceTracker(self.jit_dylib), false) }
    }

    #[llvm_versions(12.0..=latest)]
    pub fn define(&self, materialization_unit: MaterializationUnit) -> Result<(), LLVMError> {
        let result = LLVMError::new(unsafe {
            LLVMOrcJITDylibDefine(self.jit_dylib, materialization_unit.materialization_unit)
        });
        forget(materialization_unit);
        result
    }

    #[llvm_versions(12.0..=latest)]
    pub fn clear(&self) -> Result<(), LLVMError> {
        LLVMError::new(unsafe { LLVMOrcJITDylibClear(self.jit_dylib) })
    }

    pub fn add_generator() {
        todo!();
    }
}

/// A `ResourceTracker` is used to add/remove JIT resources.
/// Look at [`JITDylib`] for further information.
#[llvm_versions(12.0..=latest)]
#[derive(Debug)]
pub struct ResourceTracker<'jit> {
    rt: LLVMOrcResourceTrackerRef,
    default: bool,
    _marker: PhantomData<&'jit ()>,
}

#[llvm_versions(12.0..=latest)]
impl<'jit> ResourceTracker<'jit> {
    unsafe fn new(rt: LLVMOrcResourceTrackerRef, default: bool) -> Self {
        assert!(!rt.is_null());
        ResourceTracker {
            rt,
            default,
            _marker: PhantomData,
        }
    }

    pub fn transfer_to(&self, destination: &ResourceTracker) {
        unsafe {
            LLVMOrcResourceTrackerTransferTo(self.rt, destination.rt);
        }
    }

    pub fn remove(self) -> Result<(), LLVMError> {
        LLVMError::new(unsafe { LLVMOrcResourceTrackerRemove(self.rt) })
    }
}

#[llvm_versions(12.0..=latest)]
impl Drop for ResourceTracker<'_> {
    fn drop(&mut self) {
        if !self.default && !self.rt.is_null() {
            unsafe {
                LLVMOrcReleaseResourceTracker(self.rt);
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExecutionSession<'jit> {
    execution_session: LLVMOrcExecutionSessionRef,
    _marker: PhantomData<&'jit ()>,
}

impl<'jit> ExecutionSession<'jit> {
    unsafe fn new_borrowed(execution_session: LLVMOrcExecutionSessionRef) -> Self {
        assert!(!execution_session.is_null());
        ExecutionSession {
            execution_session,
            _marker: PhantomData,
        }
    }

    pub fn set_error_reporter() {
        todo!();
    }

    pub fn get_symbol_string_pool() {
        todo!();
    }

    pub fn intern(&self, name: &str) -> SymbolStringPoolEntry {
        unsafe {
            SymbolStringPoolEntry::new(LLVMOrcExecutionSessionIntern(
                self.execution_session,
                to_c_str(name).as_ptr(),
            ))
        }
    }

    #[llvm_versions(12.0..=latest)]
    pub fn create_bare_jit_dylib(&self, name: &str) -> JITDylib {
        unsafe {
            JITDylib::new(LLVMOrcExecutionSessionCreateBareJITDylib(
                self.execution_session,
                to_c_str(name).as_ptr(),
            ))
        }
    }

    #[llvm_versions(12.0..=latest)]
    pub fn create_jit_dylib(&self, name: &str) -> Result<JITDylib, LLVMError> {
        let mut jit_dylib = ptr::null_mut();
        unsafe {
            LLVMError::new(LLVMOrcExecutionSessionCreateJITDylib(
                self.execution_session,
                &mut jit_dylib,
                to_c_str(name).as_ptr(),
            ))?;
            Ok(JITDylib::new(jit_dylib))
        }
    }

    #[llvm_versions(12.0..=latest)]
    pub fn get_jit_dylib_by_name(&self, name: &str) -> Option<JITDylib> {
        let jit_dylib = unsafe {
            LLVMOrcExecutionSessionGetJITDylibByName(
                self.execution_session,
                to_c_str(name).as_ptr(),
            )
        };
        if jit_dylib.is_null() {
            None
        } else {
            Some(unsafe { JITDylib::new(jit_dylib) })
        }
    }

    #[llvm_versions(12.0..=latest)]
    pub fn create_rt_dyld_object_linking_layer_with_section_memory_manager(
        &self,
    ) -> RTDyldObjectLinkingLayer {
        let object_layer = unsafe {
            LLVMOrcCreateRTDyldObjectLinkingLayerWithSectionMemoryManager(self.execution_session)
        };
        RTDyldObjectLinkingLayer { object_layer }
    }
}

#[llvm_versions(12.0..=latest)]
impl_owned_ptr!(
    ObjectLayerRef,
    LLVMOrcObjectLayerRef,
    LLVMOrcDisposeObjectLayer
);

#[llvm_versions(12.0..=latest)]
#[derive(Debug)]
pub struct ObjectLayer<'jit> {
    object_layer: OwnedOrBorrowedPtr<'jit, ObjectLayerRef>,
}

#[llvm_versions(12.0..=latest)]
impl<'jit> ObjectLayer<'jit> {
    unsafe fn new_borrowed(object_layer: LLVMOrcObjectLayerRef) -> Self {
        assert!(!object_layer.is_null());
        ObjectLayer {
            object_layer: OwnedOrBorrowedPtr::borrowed(object_layer),
        }
    }

    unsafe fn new_owned(object_layer: LLVMOrcObjectLayerRef) -> Self {
        assert!(!object_layer.is_null());
        ObjectLayer {
            object_layer: OwnedOrBorrowedPtr::Owned(ObjectLayerRef(object_layer)),
        }
    }

    #[llvm_versions(13.0..=latest)]
    pub fn add_object_file(
        &self,
        jit_dylib: &JITDylib,
        object_buffer: MemoryBuffer,
    ) -> Result<(), LLVMError> {
        let result = LLVMError::new(unsafe {
            LLVMOrcObjectLayerAddObjectFile(
                self.object_layer.as_ptr(),
                jit_dylib.jit_dylib,
                object_buffer.memory_buffer,
            )
        });
        forget(object_buffer);
        result
    }

    // #[llvm_versions(14.0..=latest)]
    // pub fn add_object_file_with_rt(
    //     &self,
    //     rt: &ResourceTracker,
    //     object_buffer: MemoryBuffer,
    // ) -> Result<(), LLVMError> {
    //     let result = LLVMError::new(unsafe {
    //         LLVMOrcObjectLayerAddObjectFileWithRT(
    //             self.object_layer.as_ptr(),
    //             rt.rt,
    //             object_buffer.memory_buffer,
    //         )
    //     });
    //     forget(object_buffer);
    //     result
    // }

    #[llvm_versions(13.0..=latest)]
    pub fn emit(
        &self,
        materialization_responsibility: MaterializationResponsibility,
        object_buffer: MemoryBuffer,
    ) {
        unsafe {
            LLVMOrcObjectLayerEmit(
                self.object_layer.as_ptr(),
                materialization_responsibility.materialization_responsibility,
                object_buffer.memory_buffer,
            );
        }
    }
}

#[llvm_versions(12.0..=latest)]
impl From<RTDyldObjectLinkingLayer> for ObjectLayer<'static> {
    fn from(rt_dyld_object_linking_layer: RTDyldObjectLinkingLayer) -> Self {
        unsafe { ObjectLayer::new_borrowed(rt_dyld_object_linking_layer.object_layer) }
    }
}

/// Represents an RTDyldObjectLinkingLayer a special [`ObjectLayer`].
// Does not have a drop implementation as calling LLVMOrcDisposeObjectLayer
// on an RTDyldObjectLinkingLayer segfaults. Does leak memory!!!
#[llvm_versions(12.0..=latest)]
#[derive(Debug)]
#[repr(transparent)]
#[must_use]
pub struct RTDyldObjectLinkingLayer {
    object_layer: LLVMOrcObjectLayerRef,
}

#[llvm_versions(12.0..=latest)]
impl RTDyldObjectLinkingLayer {
    pub fn get<'a>(&'a self) -> ObjectLayer<'a> {
        unsafe { ObjectLayer::new_borrowed(self.object_layer) }
    }
}

#[llvm_versions(13.0..=latest)]
#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct IRTransformLayer<'jit> {
    ir_transform_layer: LLVMOrcIRTransformLayerRef,
    _marker: PhantomData<&'jit ()>,
}

#[llvm_versions(13.0..=latest)]
impl<'jit> IRTransformLayer<'jit> {
    unsafe fn new_borrowed(ir_transform_layer: LLVMOrcIRTransformLayerRef) -> Self {
        assert!(!ir_transform_layer.is_null());
        IRTransformLayer {
            ir_transform_layer,
            _marker: PhantomData,
        }
    }

    pub fn emit<'ctx>(
        &self,
        materialization_responsibility: MaterializationResponsibility,
        module: ThreadSafeModule<'ctx>,
    ) {
        unsafe {
            LLVMOrcIRTransformLayerEmit(
                self.ir_transform_layer,
                materialization_responsibility.materialization_responsibility,
                module.thread_safe_module,
            );
        }
        forget(module);
    }

    pub fn set_transform() {
        todo!();
    }
}

#[llvm_versions(12.0..=latest)]
#[llvm_versioned_item]
#[derive(Debug)]
pub struct MaterializationUnit {
    materialization_unit: LLVMOrcMaterializationUnitRef,
}

#[llvm_versions(12.0..=latest)]
impl MaterializationUnit {
    unsafe fn new(materialization_unit: LLVMOrcMaterializationUnitRef) -> Self {
        assert!(!materialization_unit.is_null());
        MaterializationUnit {
            materialization_unit,
        }
    }

    #[llvm_versions(13.0..=latest)]
    pub fn create<'jit>(
        name: &str,
        mut symbols: SymbolFlagsMapPairs,
        static_initializer: Option<SymbolStringPoolEntry>,
        materializer: Box<(dyn Materializer + 'jit)>,
    ) -> Self {
        unsafe {
            MaterializationUnit::new(LLVMOrcCreateCustomMaterializationUnit(
                to_c_str(name).as_ptr(),
                transmute::<*mut MaterializationUnitCtx, _>(Box::into_raw(Box::new(materializer))),
                symbols.raw_ptr(),
                symbols.len(),
                static_initializer
                    .map(|s| s.entry)
                    .unwrap_or(ptr::null_mut()),
                materialization_unit_materialize,
                materialization_unit_discard,
                materialization_unit_destroy,
            ))
        }
    }

    pub fn from_absolute_symbols(mut symbols: SymbolMapPairs) -> Self {
        unsafe {
            MaterializationUnit::new(LLVMOrcAbsoluteSymbols(symbols.raw_ptr(), symbols.len()))
        }
    }

    #[llvm_versions(13.0..=latest)]
    pub fn create_with_lazy_reexports(
        lazy_call_through_manager: LazyCallThroughManager,
        indirect_stubs_manager: IndirectStubsManager,
        source_ref: &JITDylib,
        mut callable_aliases: SymbolAliasMapPairs,
    ) -> Self {
        unsafe {
            MaterializationUnit::new(LLVMOrcLazyReexports(
                lazy_call_through_manager.lazy_call_through_manager,
                indirect_stubs_manager.indirect_stubs_manager,
                source_ref.jit_dylib,
                callable_aliases.pairs.as_mut_ptr(),
                callable_aliases.pairs.len(),
            ))
        }
    }
}

#[llvm_versions(12.0..=latest)]
impl Drop for MaterializationUnit {
    fn drop(&mut self) {
        unsafe {
            LLVMOrcDisposeMaterializationUnit(self.materialization_unit);
        }
    }
}

#[llvm_versions(13.0..=latest)]
type MaterializationUnitCtx<'jit> = Box<dyn Materializer + 'jit>;

#[llvm_versions(13.0..=latest)]
#[no_mangle]
extern "C" fn materialization_unit_materialize(
    ctx: *mut c_void,
    materialization_responsibility: LLVMOrcMaterializationResponsibilityRef,
) {
    unsafe {
        let materializer: &mut MaterializationUnitCtx = transmute(ctx);
        materializer.materialize(MaterializationResponsibility::new(
            materialization_responsibility,
        ));
        let materializer: *mut MaterializationUnitCtx = transmute(ctx);
        drop(Box::from_raw(materializer));
    }
}

#[llvm_versions(13.0..=latest)]
#[no_mangle]
extern "C" fn materialization_unit_discard(
    ctx: *mut c_void,
    jit_dylib: LLVMOrcJITDylibRef,
    symbol: LLVMOrcSymbolStringPoolEntryRef,
) {
    unsafe {
        let materializer: &mut MaterializationUnitCtx = transmute(ctx);
        let jit_dylib = JITDylib::new(jit_dylib);
        let symbol = SymbolStringPoolEntry::new(symbol);
        materializer.discard(&jit_dylib, &symbol);
        forget(jit_dylib);
        forget(symbol);
    }
}

#[llvm_versions(13.0..=latest)]
#[no_mangle]
extern "C" fn materialization_unit_destroy(ctx: *mut c_void) {
    unsafe {
        let materializer: *mut MaterializationUnitCtx = transmute(ctx);
        drop(Box::from_raw(materializer));
    }
}

#[llvm_versions(13.0..=latest)]
pub trait Materializer {
    fn materialize(&mut self, materialization_responsibility: MaterializationResponsibility);
    fn discard(&mut self, jit_dylib: &JITDylib, symbol: &SymbolStringPoolEntry);
}

#[llvm_versions(13.0..=latest)]
impl<M, D> Materializer for (M, D)
where
    M: FnMut(MaterializationResponsibility),
    D: FnMut(&JITDylib, &SymbolStringPoolEntry),
{
    fn materialize(&mut self, materialization_responsibility: MaterializationResponsibility) {
        let (materialize, _) = self;
        materialize(materialization_responsibility)
    }

    fn discard(&mut self, jit_dylib: &JITDylib, symbol: &SymbolStringPoolEntry) {
        let (_, discard) = self;
        discard(jit_dylib, symbol)
    }
}

/// Tracks responsibility for materialization. An instance is passed to
/// the [`Materializer::materialize`] function, when a symbol is requested.
#[llvm_versions(13.0..=latest)]
#[derive(Debug)]
#[must_use]
pub struct MaterializationResponsibility {
    materialization_responsibility: LLVMOrcMaterializationResponsibilityRef,
}

#[llvm_versions(13.0..=latest)]
impl MaterializationResponsibility {
    unsafe fn new(materialization_responsibility: LLVMOrcMaterializationResponsibilityRef) -> Self {
        assert!(!materialization_responsibility.is_null());
        MaterializationResponsibility {
            materialization_responsibility,
        }
    }

    /// Returns the target [`JITDylib`].
    /// ```
    /// use inkwell::orc2::{
    ///     JITDylib, MaterializationResponsibility, Materializer, SymbolStringPoolEntry
    /// };
    ///
    /// struct MyMaterializer {}
    ///
    /// impl Materializer for MyMaterializer {
    ///     fn materialize(&mut self, materialization_responsibility: MaterializationResponsibility) {
    ///         let jit_dylib = materialization_responsibility.get_target_jit_dylib();
    ///         // materialize implementation...
    ///     }
    ///
    ///     fn discard(&mut self, jit_dylib: &JITDylib, symbol: &SymbolStringPoolEntry) {
    ///         // discard implementation...
    ///     }
    /// }
    /// ```
    pub fn get_target_jit_dylib(&self) -> JITDylib {
        unsafe {
            JITDylib::new(LLVMOrcMaterializationResponsibilityGetTargetDylib(
                self.materialization_responsibility,
            ))
        }
    }

    /// Returns the [`ExecutionSession`] where the symbol is requested.
    /// ```
    /// use inkwell::orc2::{
    ///     JITDylib, MaterializationResponsibility, Materializer, SymbolStringPoolEntry
    /// };
    ///
    /// struct MyMaterializer {}
    ///
    /// impl Materializer for MyMaterializer {
    ///     fn materialize(&mut self, materialization_responsibility: MaterializationResponsibility) {
    ///         let execution_session = materialization_responsibility.get_execution_session();
    ///         // materialize implementation...
    ///     }
    ///
    ///     fn discard(&mut self, jit_dylib: &JITDylib, symbol: &SymbolStringPoolEntry) {
    ///         // discard implementation...
    ///     }
    /// }
    /// ```
    pub fn get_execution_session(&self) -> ExecutionSession {
        unsafe {
            ExecutionSession::new_borrowed(LLVMOrcMaterializationResponsibilityGetExecutionSession(
                self.materialization_responsibility,
            ))
        }
    }

    /// Returns the symbol flags map ([`SymbolFlagsMapPair`]) this instance.
    /// ```
    /// use std::iter::{self, FromIterator};
    /// use inkwell::orc2::{
    ///     lljit::LLJIT, JITDylib, MaterializationResponsibility, MaterializationUnit,
    ///     SymbolFlags, SymbolFlagsMapPairs, SymbolStringPoolEntry,
    /// };
    ///
    /// let lljit = LLJIT::create().expect("LLJIT::create failed");
    /// let symbols = SymbolFlagsMapPairs::from_iter(
    ///     vec!["main", "fourty_two"]
    ///         .into_iter()
    ///         .map(|name| lljit.mangle_and_intern(name))
    ///         .zip(iter::repeat(SymbolFlags::new(0, 0))),
    /// );
    ///
    /// let materialization_unit = MaterializationUnit::create(
    ///     "my_materialization_unit",
    ///     symbols,
    ///     None,
    ///     Box::new((
    ///         |materialization_responsibility: MaterializationResponsibility| {
    ///             let symbols = materialization_responsibility.get_symbols(); // "main", "fourty_two"
    ///             // materialize implementation...
    /// #            let mut symbols = symbols
    /// #                    .names()
    /// #                    .map(|name| name.to_string())
    /// #                    .collect::<Vec<_>>();
    /// #            symbols.sort_unstable();
    /// #            assert_eq!(symbols, vec!["fourty_two", "main"]);
    /// #            materialization_responsibility.fail_materialization();
    ///         },
    ///         |jit_dylib: &JITDylib, symbol: &SymbolStringPoolEntry| {},
    ///     )),
    /// );
    ///
    /// lljit.get_main_jit_dylib().define(materialization_unit);
    /// lljit.get_function_address("main");
    /// ```
    pub fn get_symbols(&self) -> SymbolFlagsMapPairs {
        let mut num_pairs: usize = 0;
        unsafe {
            let ptr = LLVMOrcMaterializationResponsibilityGetSymbols(
                self.materialization_responsibility,
                &mut num_pairs,
            );
            SymbolFlagsMapPairs::from_raw_parts(ptr, num_pairs)
        }
    }

    /// Returns the static initializer symbol, if present.
    /// ```
    /// use inkwell::orc2::{
    ///     JITDylib, MaterializationResponsibility, Materializer, SymbolStringPoolEntry
    /// };
    ///
    /// struct MyMaterializer {}
    ///
    /// impl Materializer for MyMaterializer {
    ///     fn materialize(&mut self, materialization_responsibility: MaterializationResponsibility) {
    ///         let initializer_symbol = materialization_responsibility.get_static_initializer_symbol();
    ///         // materialize implementation...
    ///     }
    ///
    ///     fn discard(&mut self, jit_dylib: &JITDylib, symbol: &SymbolStringPoolEntry) {
    ///         // discard implementation...
    ///     }
    /// }
    /// ```
    pub fn get_static_initializer_symbol(&self) -> Option<SymbolStringPoolEntry> {
        let ptr = unsafe {
            LLVMOrcMaterializationResponsibilityGetInitializerSymbol(
                self.materialization_responsibility,
            )
        };
        if ptr.is_null() {
            None
        } else {
            Some(unsafe { SymbolStringPoolEntry::new(ptr) })
        }
    }

    /// Return the requested symbols.
    /// ```
    /// use std::iter::{self, FromIterator};
    /// use inkwell::orc2::{
    ///     lljit::LLJIT, JITDylib, MaterializationResponsibility, MaterializationUnit,
    ///     SymbolFlags, SymbolFlagsMapPairs, SymbolStringPoolEntry,
    /// };
    ///
    /// let lljit = LLJIT::create().expect("LLJIT::create failed");
    /// let symbols = SymbolFlagsMapPairs::from_iter(
    ///     vec!["main", "fourty_two"]
    ///         .into_iter()
    ///         .map(|name| lljit.mangle_and_intern(name))
    ///         .zip(iter::repeat(SymbolFlags::new(0, 0))),
    /// );
    ///
    /// let materialization_unit = MaterializationUnit::create(
    ///     "my_materialization_unit",
    ///     symbols,
    ///     None,
    ///     Box::new((
    ///         |materialization_responsibility: MaterializationResponsibility| {
    ///             let symbols = materialization_responsibility.get_requested_symbols(); // "main"
    ///             // materialize implementation...
    /// #            assert_eq!(
    /// #                symbols
    /// #                    .as_ref()
    /// #                    .into_iter()
    /// #                    .map(|name| name.to_string())
    /// #                    .collect::<Vec<_>>(),
    /// #                vec!["main"]
    /// #            );
    /// #            materialization_responsibility.fail_materialization();
    ///         },
    ///         |jit_dylib: &JITDylib, symbol: &SymbolStringPoolEntry| {},
    ///     )),
    /// );
    ///
    /// lljit.get_main_jit_dylib().define(materialization_unit);
    /// lljit.get_function_address("main");
    /// ```
    pub fn get_requested_symbols(&self) -> SymbolStringPoolEntries {
        let mut num_symbols = 0;
        unsafe {
            let ptr = LLVMOrcMaterializationResponsibilityGetRequestedSymbols(
                self.materialization_responsibility,
                &mut num_symbols,
            );

            SymbolStringPoolEntries::from_raw_parts(ptr, num_symbols)
        }
    }

    /// Notify the target [`JITDylib`] that the `symbols` have been resolved.
    /// This updates the addresses of the symbols in the [`JITDylib`].
    /// Symbols can be resolved in sub sets of all the symbols.
    /// All symbols have to be resolved when calling [`notify_emitted`](Self::notify_emitted).
    /// ```
    /// use inkwell::orc2::{
    ///     JITDylib, MaterializationResponsibility, Materializer, SymbolStringPoolEntry
    /// };
    /// # use inkwell::orc2::SymbolMapPairs;
    ///
    /// struct MyMaterializer {}
    ///
    /// impl Materializer for MyMaterializer {
    ///     fn materialize(&mut self, materialization_responsibility: MaterializationResponsibility) {
    ///         // materialize implementation...
    /// #        let all_symbols = SymbolMapPairs::new(vec![]);
    ///         materialization_responsibility.notify_resolved(all_symbols);
    ///         materialization_responsibility.notify_emitted();
    ///     }
    ///
    ///     fn discard(&mut self, jit_dylib: &JITDylib, symbol: &SymbolStringPoolEntry) {
    ///         // discard implementation...
    ///     }
    /// }
    /// ```
    /// In case of an error [`fail_materialization`](Self::fail_materialization) should be called.
    pub fn notify_resolved(&self, mut symbols: SymbolMapPairs) -> Result<(), LLVMError> {
        LLVMError::new(unsafe {
            LLVMOrcMaterializationResponsibilityNotifyResolved(
                self.materialization_responsibility,
                symbols.raw_ptr(),
                symbols.len(),
            )
        })
    }

    /// Notify the [`JITDylib`] that all symbols have been emitted.
    /// This method will return an [`LLVMError`] if any symbols being resolved
    /// have been moved to the error state due to the failure of a dependency.
    /// ```
    /// use inkwell::orc2::{
    ///     JITDylib, MaterializationResponsibility, Materializer, SymbolStringPoolEntry
    /// };
    ///
    /// struct MyMaterializer {}
    ///
    /// impl Materializer for MyMaterializer {
    ///     fn materialize(&mut self, materialization_responsibility: MaterializationResponsibility) {
    ///         // materialize implementation...
    ///         if let Err((materialization_responsibility, error)) =
    ///             materialization_responsibility.notify_emitted()
    ///         {
    ///             // Log the error...
    ///             materialization_responsibility.fail_materialization();
    ///         }
    ///     }
    ///
    ///     fn discard(&mut self, jit_dylib: &JITDylib, symbol: &SymbolStringPoolEntry) {
    ///         // discard implementation...
    ///     }
    /// }
    /// ```
    pub fn notify_emitted(self) -> Result<(), (Self, LLVMError)> {
        LLVMError::new(unsafe {
            LLVMOrcMaterializationResponsibilityNotifyEmitted(self.materialization_responsibility)
        })
        .map_err(|err| (self, err))
    }

    /// Attempt to claim responsibility for new definitions. This is method is
    /// usefull if new symbols are added by the materialization unit in the
    /// compilation process.
    /// ```
    /// use inkwell::orc2::{
    ///     JITDylib, MaterializationResponsibility, Materializer, SymbolStringPoolEntry
    /// };
    /// # use inkwell::orc2::SymbolFlagsMapPairs;
    ///
    /// struct MyMaterializer {}
    ///
    /// impl Materializer for MyMaterializer {
    ///     fn materialize(&mut self, materialization_responsibility: MaterializationResponsibility) {
    ///         // materialize implementation generating new_symbols...
    /// #        let new_symbols = SymbolFlagsMapPairs::new(vec![]);
    ///         materialization_responsibility.define_materializing(new_symbols);
    ///         // finish materialize implementation...
    ///     }
    ///
    ///     fn discard(&mut self, jit_dylib: &JITDylib, symbol: &SymbolStringPoolEntry) {
    ///         // discard implementation...
    ///     }
    /// }
    /// ```
    pub fn define_materializing(&self, mut pairs: SymbolFlagsMapPairs) -> Result<(), LLVMError> {
        LLVMError::new(unsafe {
            LLVMOrcMaterializationResponsibilityDefineMaterializing(
                self.materialization_responsibility,
                pairs.raw_ptr(),
                pairs.len(),
            )
        })
    }

    /// Notify all not-yet-emitted covered by this instance that an
    /// error has occurred.
    /// This will remove all symbols covered by this `MaterializationResponsibility`
    /// from the target [`JITDylib`], and send an error to any queries
    /// waiting on these symbols.
    /// ```
    /// use inkwell::orc2::{
    ///     JITDylib, MaterializationResponsibility, Materializer, SymbolStringPoolEntry
    /// };
    ///
    /// struct MyMaterializer {}
    ///
    /// impl Materializer for MyMaterializer {
    ///     fn materialize(&mut self, materialization_responsibility: MaterializationResponsibility) {
    ///         // materialize implementation...
    ///         materialization_responsibility.fail_materialization();
    ///     }
    ///
    ///     fn discard(&mut self, jit_dylib: &JITDylib, symbol: &SymbolStringPoolEntry) {
    ///         // discard implementation...
    ///     }
    /// }
    /// ```
    pub fn fail_materialization(&self) {
        unsafe {
            LLVMOrcMaterializationResponsibilityFailMaterialization(
                self.materialization_responsibility,
            );
        }
    }

    /// Transfers responsibility to `materialization_unit` for all symbols
    /// defined by `materialization_unit`.This allows materializers to
    /// break up work based on run-time information (e.g. by introspecting
    /// which symbols have actually been looked up and materializing only those).
    /// ```
    /// use std::{collections::HashSet, iter::FromIterator};
    ///
    /// use inkwell::orc2::{
    ///     JITDylib, MaterializationResponsibility, MaterializationUnit, Materializer,
    ///     SymbolFlagsMapPairs, SymbolStringPoolEntry,
    /// };
    ///
    /// #[derive(Debug, Clone)]
    /// struct MaterializeOnlyRequested<M> {
    ///     materializer: M,
    ///     static_initializer_symbol: Option<SymbolStringPoolEntry>,
    /// }
    ///
    /// impl<M> Materializer for MaterializeOnlyRequested<M>
    /// where
    ///     M: Materializer + Clone,
    /// {
    ///     fn materialize(&mut self, materialization_responsibility: MaterializationResponsibility) {
    ///         let requested_symbols: HashSet<_> = HashSet::from_iter(
    ///             materialization_responsibility
    ///                 .get_requested_symbols()
    ///                 .as_ref()
    ///                 .into_iter()
    ///                 .cloned(),
    ///         );
    ///         let not_requested_symbols: Vec<_> = materialization_responsibility
    ///             .get_symbols()
    ///             .into_iter()
    ///             .filter(|symbol| !requested_symbols.contains(symbol.get_name()))
    ///             .collect();
    ///
    ///         // Create new materialization unit for not requested symbols
    ///         if !not_requested_symbols.is_empty() {
    ///             let materialization_unit = MaterializationUnit::create(
    ///                 "compile_only_requested",
    ///                 SymbolFlagsMapPairs::new(not_requested_symbols),
    ///                 self.static_initializer_symbol.clone(),
    ///                 Box::new(self.clone()),
    ///             );
    ///             if let Err(error) = materialization_responsibility.replace(materialization_unit) {
    ///                 // Log error...
    ///                 eprintln!("{}", error.get_message());
    ///                 materialization_responsibility.fail_materialization();
    ///                 return;
    ///             }
    ///         }
    ///         self.materializer
    ///             .materialize(materialization_responsibility);
    ///     }
    ///
    ///     fn discard(&mut self, _jit_dylib: &JITDylib, _symbol: &SymbolStringPoolEntry) {}
    /// }
    /// ```
    pub fn replace(&self, materialization_unit: MaterializationUnit) -> Result<(), LLVMError> {
        let result = LLVMError::new(unsafe {
            LLVMOrcMaterializationResponsibilityReplace(
                self.materialization_responsibility,
                materialization_unit.materialization_unit,
            )
        });
        forget(materialization_unit);
        result
    }

    /// Delegates responsibility for the given `symbols` to the returned
    /// `MaterializationResponsibility`. Useful for breaking up work between
    /// threads, or different kinds of materialization processes.
    pub fn delegate(
        &self,
        mut symbols: Vec<SymbolStringPoolEntry>,
    ) -> Result<MaterializationResponsibility, LLVMError> {
        let mut ptr = ptr::null_mut();
        LLVMError::new(unsafe {
            LLVMOrcMaterializationResponsibilityDelegate(
                self.materialization_responsibility,
                transmute(symbols.as_mut_ptr()),
                symbols.len(),
                &mut ptr,
            )
        })?;
        Ok(unsafe { MaterializationResponsibility::new(ptr) })
    }

    /// Adds dependencies to a symbol `name` that the `MaterializationResponsibility`.
    pub fn add_dependencies(
        &self,
        name: SymbolStringPoolEntry,
        mut dependencies: DependenceMapPairs,
    ) {
        unsafe {
            LLVMOrcMaterializationResponsibilityAddDependencies(
                self.materialization_responsibility,
                name.entry,
                dependencies.raw_ptr(),
                dependencies.len(),
            )
        }
    }

    /// Adds dependencies to all symbols of the `MaterializationResponsibility`.
    pub fn add_dependencies_for_all(&self, mut dependencies: DependenceMapPairs) {
        unsafe {
            LLVMOrcMaterializationResponsibilityAddDependenciesForAll(
                self.materialization_responsibility,
                dependencies.raw_ptr(),
                dependencies.len(),
            );
        }
    }
}

#[llvm_versions(13.0..=latest)]
#[derive(Debug)]
pub struct SymbolFlagsMapPairs {
    pairs: Vec<SymbolFlagsMapPair>,
}

#[llvm_versions(13.0..=latest)]
impl SymbolFlagsMapPairs {
    unsafe fn from_raw_parts(ptr: LLVMOrcCSymbolFlagsMapPairs, num_pairs: usize) -> Self {
        SymbolFlagsMapPairs {
            pairs: Vec::from_raw_parts(transmute(ptr), num_pairs, num_pairs),
        }
    }

    pub fn new(pairs: Vec<SymbolFlagsMapPair>) -> Self {
        SymbolFlagsMapPairs { pairs }
    }

    fn raw_ptr(&mut self) -> LLVMOrcCSymbolFlagsMapPairs {
        unsafe { transmute(self.pairs.as_mut_ptr()) }
    }

    pub fn len(&self) -> usize {
        self.pairs.len()
    }

    pub fn names_iter(&self) -> impl Iterator<Item = &SymbolStringPoolEntry> {
        self.pairs.iter().map(|pair| pair.get_name())
    }

    pub fn names(self) -> impl Iterator<Item = SymbolStringPoolEntry> {
        self.pairs.into_iter().map(|pair| pair.name())
    }

    pub fn flags_iter(&self) -> impl Iterator<Item = &SymbolFlags> {
        self.pairs.iter().map(|pair| pair.get_flags())
    }

    pub fn flags(self) -> impl Iterator<Item = SymbolFlags> {
        self.pairs.into_iter().map(|pair| pair.flags())
    }

    pub fn iter(&self) -> impl Iterator<Item = &SymbolFlagsMapPair> {
        self.pairs.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut SymbolFlagsMapPair> {
        self.pairs.iter_mut()
    }
}

#[llvm_versions(13.0..=latest)]
impl IntoIterator for SymbolFlagsMapPairs {
    type Item = SymbolFlagsMapPair;

    type IntoIter = vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.pairs.into_iter()
    }
}

#[llvm_versions(13.0..=latest)]
impl FromIterator<(SymbolStringPoolEntry, SymbolFlags)> for SymbolFlagsMapPairs {
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = (SymbolStringPoolEntry, SymbolFlags)>,
    {
        SymbolFlagsMapPairs::new(
            iter.into_iter()
                .map(|(name, flags)| SymbolFlagsMapPair::new(name, flags))
                .collect(),
        )
    }
}

#[llvm_versions(13.0..=latest)]
impl FromIterator<SymbolFlagsMapPair> for SymbolFlagsMapPairs {
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = SymbolFlagsMapPair>,
    {
        SymbolFlagsMapPairs::new(iter.into_iter().collect())
    }
}

#[llvm_versions(13.0..=latest)]
#[repr(transparent)]
pub struct SymbolFlagsMapPair {
    pair: LLVMOrcCSymbolFlagsMapPair,
}

#[llvm_versions(13.0..=latest)]
impl SymbolFlagsMapPair {
    pub fn new(name: SymbolStringPoolEntry, flags: SymbolFlags) -> Self {
        Self {
            pair: LLVMOrcCSymbolFlagsMapPair {
                Name: unsafe { transmute(name) },
                Flags: unsafe { transmute(flags) },
            },
        }
    }

    pub fn get_name(&self) -> &SymbolStringPoolEntry {
        unsafe { transmute(&self.pair.Name) }
    }

    pub fn name(self) -> SymbolStringPoolEntry {
        self.destruct().0
    }

    pub fn get_flags(&self) -> &SymbolFlags {
        unsafe { transmute(&self.pair.Flags) }
    }

    pub fn flags(self) -> SymbolFlags {
        self.destruct().1
    }

    pub fn destruct(self) -> (SymbolStringPoolEntry, SymbolFlags) {
        unsafe { (transmute(self.pair.Name), transmute(self.pair.Flags)) }
    }
}

#[llvm_versions(13.0..=latest)]
impl fmt::Debug for SymbolFlagsMapPair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SymbolFlagsMapPair")
            .field("name", self.get_name())
            .field("flags", self.get_flags())
            .finish()
    }
}

#[llvm_versions(12.0..=latest)]
#[repr(transparent)]
pub struct SymbolFlags {
    flags: LLVMJITSymbolFlags,
}

#[llvm_versions(12.0..=latest)]
impl SymbolFlags {
    pub fn new(generic_flags: u8, target_flags: u8) -> Self {
        SymbolFlags {
            flags: LLVMJITSymbolFlags {
                GenericFlags: generic_flags,
                TargetFlags: target_flags,
            },
        }
    }

    pub fn get_generic_flags(&self) -> u8 {
        self.flags.GenericFlags
    }

    pub fn get_target_flags(&self) -> u8 {
        self.flags.TargetFlags
    }
}

#[llvm_versions(12.0..=latest)]
impl Clone for SymbolFlags {
    fn clone(&self) -> Self {
        SymbolFlags::new(self.flags.GenericFlags, self.flags.TargetFlags)
    }
}

#[llvm_versions(12.0..=latest)]
impl fmt::Debug for SymbolFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SymbolFlags")
            .field("generic_flags", &self.get_generic_flags())
            .field("target_flags", &self.get_target_flags())
            .finish()
    }
}

#[llvm_versions(12.0..=latest)]
#[derive(Debug)]
pub struct SymbolMapPairs {
    pairs: Vec<SymbolMapPair>,
}

#[llvm_versions(12.0..=latest)]
impl SymbolMapPairs {
    pub fn new(pairs: Vec<SymbolMapPair>) -> Self {
        SymbolMapPairs { pairs }
    }

    unsafe fn raw_ptr(&mut self) -> LLVMOrcCSymbolMapPairs {
        transmute(self.pairs.as_mut_ptr())
    }

    pub fn len(&self) -> usize {
        self.pairs.len()
    }

    pub fn names(self) -> impl Iterator<Item = SymbolStringPoolEntry> {
        self.pairs.into_iter().map(|pair| pair.name())
    }

    pub fn names_iter(&self) -> impl Iterator<Item = &SymbolStringPoolEntry> {
        self.pairs.iter().map(|pair| pair.get_name())
    }

    pub fn symbols(self) -> impl Iterator<Item = EvaluatedSymbol> {
        self.pairs.into_iter().map(|pair| pair.symbol())
    }

    pub fn symbols_iter(&self) -> impl Iterator<Item = &EvaluatedSymbol> {
        self.pairs.iter().map(|pair| pair.get_symbol())
    }

    pub fn iter(&self) -> impl Iterator<Item = &SymbolMapPair> {
        self.pairs.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut SymbolMapPair> {
        self.pairs.iter_mut()
    }
}

#[llvm_versions(12.0..=latest)]
impl IntoIterator for SymbolMapPairs {
    type Item = SymbolMapPair;

    type IntoIter = vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.pairs.into_iter()
    }
}

#[llvm_versions(12.0..=latest)]
impl FromIterator<(SymbolStringPoolEntry, EvaluatedSymbol)> for SymbolMapPairs {
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = (SymbolStringPoolEntry, EvaluatedSymbol)>,
    {
        SymbolMapPairs::new(
            iter.into_iter()
                .map(|(name, symbol)| SymbolMapPair::new(name, symbol))
                .collect(),
        )
    }
}

#[llvm_versions(12.0..=latest)]
impl FromIterator<SymbolMapPair> for SymbolMapPairs {
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = SymbolMapPair>,
    {
        SymbolMapPairs::new(iter.into_iter().collect())
    }
}

#[llvm_versions(12.0..=latest)]
#[repr(transparent)]
pub struct SymbolMapPair {
    pair: LLVMJITCSymbolMapPair,
}

#[llvm_versions(12.0..=latest)]
impl SymbolMapPair {
    pub fn new(name: SymbolStringPoolEntry, symbol: EvaluatedSymbol) -> Self {
        SymbolMapPair {
            pair: LLVMJITCSymbolMapPair {
                Name: unsafe { transmute(name) },
                Sym: unsafe { transmute(symbol) },
            },
        }
    }

    pub fn get_name(&self) -> &SymbolStringPoolEntry {
        unsafe { transmute(&self.pair.Name) }
    }

    pub fn name(self) -> SymbolStringPoolEntry {
        self.destruct().0
    }

    pub fn get_symbol(&self) -> &EvaluatedSymbol {
        unsafe { transmute(&self.pair.Sym) }
    }

    pub fn symbol(self) -> EvaluatedSymbol {
        self.destruct().1
    }

    pub fn destruct(self) -> (SymbolStringPoolEntry, EvaluatedSymbol) {
        unsafe { (transmute(self.pair.Name), transmute(self.pair.Sym)) }
    }
}

#[llvm_versions(12.0..=latest)]
impl fmt::Debug for SymbolMapPair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SymbolMapPair")
            .field("name", self.get_name())
            .field("symbol", self.get_symbol())
            .finish()
    }
}

#[llvm_versions(12.0..=latest)]
#[repr(transparent)]
pub struct EvaluatedSymbol {
    symbol: LLVMJITEvaluatedSymbol,
}

#[llvm_versions(12.0..=latest)]
impl EvaluatedSymbol {
    pub fn new(address: u64, flags: SymbolFlags) -> Self {
        EvaluatedSymbol {
            symbol: LLVMJITEvaluatedSymbol {
                Address: unsafe { transmute(address) },
                Flags: unsafe { transmute(flags) },
            },
        }
    }

    pub fn get_address(&self) -> u64 {
        self.symbol.Address
    }

    pub fn get_flags(&self) -> &SymbolFlags {
        unsafe { transmute(&self.symbol.Flags) }
    }
}

#[llvm_versions(12.0..=latest)]
impl fmt::Debug for EvaluatedSymbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EvaluatedSymbol")
            .field("address", &self.get_address())
            .field("flags", self.get_flags())
            .finish()
    }
}

#[llvm_versions(13.0..=latest)]
#[derive(Debug)]
pub struct SymbolAliasMapPairs {
    pairs: Vec<LLVMOrcCSymbolAliasMapPair>,
}

#[llvm_versions(13.0..=latest)]
#[derive(Debug)]
pub struct DependenceMapPairs<'jit> {
    pairs: Vec<DependenceMapPair<'jit>>,
}

#[llvm_versions(13.0..=latest)]
impl<'jit> DependenceMapPairs<'jit> {
    pub fn new(pairs: Vec<DependenceMapPair<'jit>>) -> Self {
        DependenceMapPairs { pairs }
    }

    unsafe fn raw_ptr(&mut self) -> LLVMOrcCDependenceMapPairs {
        transmute(self.pairs.as_mut_ptr())
    }

    pub fn len(&self) -> usize {
        self.pairs.len()
    }

    pub fn jit_dylibs(self) -> impl Iterator<Item = JITDylib<'jit>> {
        self.pairs.into_iter().map(|pair| pair.jit_dylib())
    }

    pub fn jit_dylibs_iter(&self) -> impl Iterator<Item = &SymbolStringPoolEntry> {
        self.pairs.iter().map(|pair| pair.get_jit_dylib())
    }

    pub fn names(self) -> impl Iterator<Item = Vec<SymbolStringPoolEntry>> {
        let iter: vec::IntoIter<DependenceMapPair<'_>> =
            unsafe { transmute(self.pairs.into_iter()) };
        iter.map(|pair| pair.names())
    }

    pub fn names_iter(&self) -> impl Iterator<Item = &[SymbolStringPoolEntry]> {
        self.pairs.iter().map(|pair| pair.get_names())
    }

    pub fn iter(&self) -> impl Iterator<Item = &DependenceMapPair> {
        self.pairs.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut DependenceMapPair<'jit>> {
        self.pairs.iter_mut()
    }
}

#[llvm_versions(13.0..=latest)]
impl<'jit> IntoIterator for DependenceMapPairs<'jit> {
    type Item = DependenceMapPair<'jit>;

    type IntoIter = vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.pairs.into_iter()
    }
}

#[llvm_versions(13.0..=latest)]
impl<'jit> FromIterator<(JITDylib<'jit>, Vec<SymbolStringPoolEntry>)> for DependenceMapPairs<'jit> {
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = (JITDylib<'jit>, Vec<SymbolStringPoolEntry>)>,
    {
        DependenceMapPairs::new(
            iter.into_iter()
                .map(|(jit_dylib, names)| DependenceMapPair::new(jit_dylib, names))
                .collect(),
        )
    }
}

#[llvm_versions(13.0..=latest)]
impl<'jit> FromIterator<DependenceMapPair<'jit>> for DependenceMapPairs<'jit> {
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = DependenceMapPair<'jit>>,
    {
        DependenceMapPairs::new(iter.into_iter().collect())
    }
}

#[llvm_versions(13.0..=latest)]
#[repr(transparent)]
pub struct DependenceMapPair<'jit> {
    pair: LLVMOrcCDependenceMapPair,
    _marker: PhantomData<&'jit ()>,
}

#[llvm_versions(13.0..=latest)]
impl<'jit> DependenceMapPair<'jit> {
    pub fn new(jit_dylib: JITDylib<'jit>, mut names: Vec<SymbolStringPoolEntry>) -> Self {
        names.shrink_to_fit();
        DependenceMapPair {
            pair: LLVMOrcCDependenceMapPair {
                JD: unsafe { transmute(jit_dylib) },
                Names: LLVMOrcCSymbolsList {
                    Symbols: unsafe { transmute(names.as_mut_ptr()) },
                    Length: names.len(),
                },
            },
            _marker: PhantomData,
        }
    }

    pub fn get_jit_dylib(&self) -> &SymbolStringPoolEntry {
        unsafe { transmute(&self.pair.JD) }
    }

    pub fn jit_dylib(self) -> JITDylib<'jit> {
        self.destruct().0
    }

    pub fn get_names(&self) -> &[SymbolStringPoolEntry] {
        unsafe { slice::from_raw_parts(transmute(self.pair.Names.Symbols), self.pair.Names.Length) }
    }

    pub fn names(self) -> Vec<SymbolStringPoolEntry> {
        self.destruct().1
    }

    pub fn destruct(self) -> (JITDylib<'jit>, Vec<SymbolStringPoolEntry>) {
        unsafe {
            (
                transmute(self.pair.JD),
                Vec::from_raw_parts(
                    transmute(self.pair.Names.Symbols),
                    self.pair.Names.Length,
                    self.pair.Names.Length,
                ),
            )
        }
    }
}

#[llvm_versions(13.0..=latest)]
impl fmt::Debug for DependenceMapPair<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DependenceMapPair")
            .field("jit_dylib", &self.get_jit_dylib())
            .field("names", &self.get_names())
            .finish()
    }
}

#[llvm_versions(13.0..=latest)]
impl Drop for DependenceMapPair<'_> {
    fn drop(&mut self) {
        unsafe {
            Vec::from_raw_parts(
                self.pair.Names.Symbols,
                self.pair.Names.Length,
                self.pair.Names.Length,
            );
        }
    }
}

#[derive(Eq, PartialEq, Hash)]
#[repr(transparent)]
pub struct SymbolStringPoolEntry {
    entry: LLVMOrcSymbolStringPoolEntryRef,
}

impl SymbolStringPoolEntry {
    unsafe fn new(entry: LLVMOrcSymbolStringPoolEntryRef) -> Self {
        assert!(!entry.is_null());
        SymbolStringPoolEntry { entry }
    }

    #[llvm_versions(12.0..=latest)]
    pub fn get_string(&self) -> &CStr {
        unsafe { CStr::from_ptr(LLVMOrcSymbolStringPoolEntryStr(self.entry)) }
    }
}

#[llvm_versions(12.0..=latest)]
impl Clone for SymbolStringPoolEntry {
    fn clone(&self) -> Self {
        unsafe {
            LLVMOrcRetainSymbolStringPoolEntry(self.entry);
        }
        Self { entry: self.entry }
    }
}

impl fmt::Debug for SymbolStringPoolEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        #[cfg(feature = "llvm11-0")]
        return f
            .debug_struct("SymbolStringPoolEntry")
            .field("entry", &self.entry)
            .finish();
        #[cfg(not(feature = "llvm11-0"))]
        return f
            .debug_struct("SymbolStringPoolEntry")
            .field("entry", &self.get_string())
            .field("ptr", &self.entry)
            .finish();
    }
}

#[llvm_versions(12.0..=latest)]
impl fmt::Display for SymbolStringPoolEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.get_string().to_string_lossy().fmt(f)
    }
}

impl Drop for SymbolStringPoolEntry {
    fn drop(&mut self) {
        unsafe {
            LLVMOrcReleaseSymbolStringPoolEntry(self.entry);
        }
    }
}

#[llvm_versions(13.0..=latest)]
pub struct SymbolStringPoolEntries {
    entries: *mut LLVMOrcSymbolStringPoolEntryRef,
    len: usize,
}

#[llvm_versions(13.0..=latest)]
impl SymbolStringPoolEntries {
    unsafe fn from_raw_parts(entries: *mut LLVMOrcSymbolStringPoolEntryRef, len: usize) -> Self {
        assert!(!entries.is_null());
        SymbolStringPoolEntries { entries, len }
    }
}

#[llvm_versions(13.0..=latest)]
impl AsRef<[SymbolStringPoolEntry]> for SymbolStringPoolEntries {
    fn as_ref(&self) -> &[SymbolStringPoolEntry] {
        unsafe { slice::from_raw_parts(transmute(self.entries), self.len) }
    }
}

#[llvm_versions(13.0..=latest)]
impl AsMut<[SymbolStringPoolEntry]> for SymbolStringPoolEntries {
    fn as_mut(&mut self) -> &mut [SymbolStringPoolEntry] {
        unsafe { slice::from_raw_parts_mut(transmute(self.entries), self.len) }
    }
}

#[llvm_versions(13.0..=latest)]
impl fmt::Debug for SymbolStringPoolEntries {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_ref().fmt(f)
    }
}

#[llvm_versions(13.0..=latest)]
impl Drop for SymbolStringPoolEntries {
    fn drop(&mut self) {
        unsafe {
            LLVMOrcDisposeSymbols(self.entries);
        }
    }
}

#[llvm_versions(13.0..=latest)]
#[derive(Debug)]
pub struct LazyCallThroughManager {
    lazy_call_through_manager: LLVMOrcLazyCallThroughManagerRef,
}

#[llvm_versions(13.0..=latest)]
#[derive(Debug)]
pub struct IndirectStubsManager {
    indirect_stubs_manager: LLVMOrcIndirectStubsManagerRef,
}