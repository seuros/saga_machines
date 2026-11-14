# frozen_string_literal: true

require_relative 'test_helper'

# The values are sealed, and sealing stops accidents, not reflection: the
# extension is the authority, and every value it reads back must be exactly
# its class and valid by the core, whatever built it.
class SealTest < Minitest::Test
  include SagaHelpers

  VALUES = [SagaId, AttemptId, ActionKey, Action, Forward, Compensating, Settlement, Snapshot].freeze

  # A named subclass raises too, but Ruby binds the constant before
  # `inherited` runs: the class leaks, and can be reopened to skip the check.
  def leak(value)
    name = :"Leak#{value.name.split('::').last}"
    return self.class.const_get(name) if self.class.const_defined?(name, false)

    assert_raises(TypeError) { self.class.class_eval("class #{name} < #{value}; end", __FILE__, __LINE__) }
    self.class.const_get(name).tap do |leaked|
      leaked.class_eval { def initialize(**fields) = Data.instance_method(:initialize).bind_call(self, **fields) }
    end
  end

  def test_values_cannot_be_subclassed_or_allocated
    VALUES.each do |value|
      assert_raises(TypeError) { Class.new(value) { def initialize(**) = nil } }
      assert_operator leak(value), :<, value
      assert_raises(NoMethodError) { value.allocate }
    end
    assert_raises(RangeError) { AttemptId.new(1).with(raw: 0) }
  end

  # A forged value never reaches the core and never earns a refusal symbol
  # (:stale, :mismatch): a leaked subclass is TypeError, a field
  # `Marshal.load` rewrote RangeError.
  def test_forged_values_are_refused_where_the_extension_reads_them
    fake_id = leak(SagaId).new(raw: -1)
    assert_raises(TypeError) { Saga.new(2, fake_id) }
    assert_raises(TypeError) { ActionKey.new(fake_id, 0, :forward) }

    saga = Saga.new(2, ID)
    action = saga.next_action
    fake_attempt = leak(AttemptId).new(raw: 0)
    fake_key = leak(ActionKey).new(saga: ID, step: 0, kind: :forward)
    fake_action = leak(Action).new(key: action.key, attempt: action.attempt)
    zero = Marshal.load(Marshal.dump(action).sub("i\x06", "i\x00"))
    assert_equal 0, zero.attempt.raw
    assert_unchanged(saga) do
      assert_raises(TypeError) { saga.resolve(fake_action, :success) }
      assert_raises(RangeError) { saga.resolve(zero, :success) }
    end
    assert_raises(TypeError) { Action.new(action.key, fake_attempt) }
    assert_raises(TypeError) { Action.new(fake_key, action.attempt) }
    assert_raises(TypeError) { Settlement.new(fake_action, :success) }
    assert_raises(RangeError) { Settlement.new(zero, :success) }
    assert_raises(TypeError) { Snapshot.new(ID, 0, leak(Forward).new(step: 0), 0, nil) }
    assert_equal :resolved, saga.resolve(action, :success)
  end

  def test_a_forged_snapshot_is_refused
    saga = Saga.with_scope(2, ID, 4)
    run_step(saga, :success)
    snapshot = saga.snapshot
    assert_raises(TypeError) { Saga.restored(2, leak(Snapshot).new(**snapshot.to_h), 4) }
    moved = Marshal.load(Marshal.dump(snapshot).sub("i\x09", "i\x0A"))
    assert_equal 5, moved.scope
    assert_raises(RangeError) { Saga.restored(2, moved, 5) }
    assert_equal snapshot, Saga.restored(2, snapshot, 4).snapshot
  end

  # A singleton class opened on a genuine value does not change what it is,
  # a clone of one included, wherever the extension reads it back.
  def test_an_opened_singleton_class_is_still_the_gems_value
    opened = ->(value) { value.clone.tap(&:singleton_class) }
    saga = Saga.with_scope(2, opened.(ID), 4)
    action = saga.next_action
    key = ActionKey.new(opened.(action.key.saga), action.key.step, action.key.kind)
    reported = opened.(Action.new(opened.(key), opened.(action.attempt)))
    assert_equal :resolved, saga.resolve(reported, :success)
    snapshot = saga.snapshot
    settled = Settlement.new(opened.(snapshot.settled.action), snapshot.settled.outcome)
    rebuilt = Snapshot.new(opened.(snapshot.id), snapshot.scope, opened.(snapshot.progress), snapshot.issued,
                           opened.(settled))
    assert_equal snapshot, Saga.restored(2, opened.(rebuilt), 4).snapshot
  end

  # The extension reads every value's fields from the struct, never through
  # its readers: a redefined reader is ignored, and cannot re-enter.
  def test_a_redefined_reader_is_ignored
    saga = Saga.with_scope(2, ID, 4)
    action = saga.next_action
    readers = { SagaId => %i[raw], AttemptId => %i[raw], ActionKey => %i[saga step kind], Action => %i[key attempt],
                Settlement => %i[action outcome], Forward => %i[step],
                Snapshot => %i[id scope progress issued settled] }
    originals = readers.flat_map { |klass, names| names.map { [klass, it, klass.instance_method(it)] } }
    begin
      originals.each { |klass, name, _| klass.define_method(name) { raise "#{name} reader ran" } }
      resolved = saga.resolve(action, :success)
    ensure
      originals.each { |klass, name, reader| klass.define_method(name, reader) }
    end
    assert_equal :resolved, resolved
    snapshot = saga.snapshot
    begin
      originals.each { |klass, name, _| klass.define_method(name) { raise "#{name} reader ran" } }
      restored = Saga.restored(2, snapshot, 4)
    ensure
      originals.each { |klass, name, reader| klass.define_method(name, reader) }
    end
    assert_equal snapshot, restored.snapshot
  end

  # The classes the extension resolved at init are pinned: after a
  # compaction that moves what it can, every path that builds or reads a
  # value still finds them.
  def test_resolved_classes_survive_compaction
    GC.verify_compaction_references(expand_heap: true, toward: :empty) if GC.respond_to?(:verify_compaction_references)
    GC.compact if GC.respond_to?(:compact)
    saga = Saga.with_scope(2, ID, 4)
    action = saga.next_action
    assert_instance_of Action, action
    assert_equal :resolved, saga.resolve(action, :success)
    assert_equal saga.snapshot, Saga.restored(2, saga.snapshot, 4).snapshot
    assert_equal Settlement.new(action, :success), saga.settled
  end
end
